use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{
    CancelToken, CancelView, Cause, CreateError, CreateFuture, Edge, Event, Fields, Graph,
    GraphBuilder, Instance, InstanceId, Node, PROGRAMMATIC, Reason, State, Target,
};
use br_graph_executor::{
    Checkpoint, CheckpointSink, Observation, Outcome, OutcomeKind, RunConfig, RunFailure,
    SinkFuture, start,
};
use tokio::sync::Notify;

use crate::host::within;
use crate::observer::Recorder;
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy)]
enum Answer {
    Succeed,
    Fail,
    Panic,
}

struct Plan {
    script: VecDeque<Answer>,
    then: Answer,
}

thread_local! {
    static PLAN: RefCell<Plan> = const {
        RefCell::new(Plan { script: VecDeque::new(), then: Answer::Succeed })
    };
}

fn plan(script: &[Answer], then: Answer) {
    PLAN.with(|plan| {
        *plan.borrow_mut() = Plan {
            script: script.iter().copied().collect(),
            then,
        }
    });
}

fn next_answer() -> Answer {
    PLAN.with(|plan| {
        let mut plan = plan.borrow_mut();
        let then = plan.then;
        plan.script.pop_front().unwrap_or(then)
    })
}

#[derive(Debug, Clone, PartialEq)]
struct Flaky(u32);

impl Serialize for Flaky {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match next_answer() {
            Answer::Succeed => serializer.serialize_u32(self.0),
            Answer::Panic => panic!("serialization panics"),
            Answer::Fail => Err(S::Error::custom("serialization exhausted")),
        }
    }
}

impl<'de> Deserialize<'de> for Flaky {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u32::deserialize(deserializer)
            .map(Flaky)
            .map_err(D::Error::custom)
    }
}

impl Fields for Flaky {
    type Update = ();

    fn empty_update() {}

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, _update: ()) -> Result<(), Reason> {
        Ok(())
    }
}

enum Behavior {
    Fail,
    Result,
    ResultAfter(Arc<Notify>),
}

struct ScriptedNode {
    name: &'static str,
    behavior: Behavior,
}

impl Node<Flaky> for ScriptedNode {
    fn name(&self) -> &str {
        self.name
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "worker"
    }

    fn create(&self, _cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Flaky> {
        let release = match &self.behavior {
            Behavior::ResultAfter(gate) => Some(Arc::clone(gate)),
            _ => None,
        };
        let fails = matches!(self.behavior, Behavior::Fail);
        Box::pin(async move {
            let (instance, mut reader, sender) = Instance::<Flaky>::channel(id);
            tokio::spawn(async move {
                let _ = reader.next().await;
                if let Some(gate) = release {
                    gate.notified().await;
                }
                sender.finish(if fails {
                    Event::failed("boom")
                } else {
                    Event::Result(())
                });
            });
            Ok::<_, CreateError>(instance)
        })
    }
}

struct EndEdge(&'static str);

impl Edge<Flaky> for EndEdge {
    fn name(&self) -> &str {
        self.0
    }

    fn source(&self) -> &str {
        self.0
    }

    fn targets(&self) -> &[Target] {
        &[Target::End]
    }

    fn evaluate(&self, _state: &State<Flaky>) -> Result<Vec<Target>, Cause> {
        Ok(vec![Target::End])
    }
}

struct AcceptingSink;

impl CheckpointSink for AcceptingSink {
    fn accept<'a>(&'a self, _checkpoint: &'a Checkpoint) -> SinkFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

fn graph_of(nodes: Vec<ScriptedNode>) -> Arc<Graph<Flaky>> {
    let mut builder = GraphBuilder::<Flaky>::new("1");
    for node in nodes {
        let name = node.name;
        builder = builder
            .node(Arc::new(node))
            .edge(Arc::new(EndEdge(name)))
            .entry(name);
    }
    Arc::new(builder.build().unwrap())
}

async fn failed_run_with_exhausted_serialization(panics: bool) -> (Vec<RunFailure>, String) {
    plan(
        &[Answer::Succeed],
        if panics { Answer::Panic } else { Answer::Fail },
    );
    let graph = graph_of(vec![ScriptedNode {
        name: "a",
        behavior: Behavior::Fail,
    }]);
    let initial = State::create(Flaky(7)).unwrap();
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let outcome = start(graph, initial, vec!["a".to_owned()], config)
        .await
        .unwrap();
    match outcome {
        Outcome::RunFailed {
            failures,
            checkpoint,
        } => (failures, checkpoint.state_json().to_owned()),
        other => panic!("expected RunFailed, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_state_that_stops_serializing_fails_the_run_with_an_unreadable_checkpoint() {
    let (failures, state_json) = failed_run_with_exhausted_serialization(false).await;
    assert!(matches!(failures[0], RunFailure::NodeFailed { .. }));
    assert!(matches!(failures[1], RunFailure::CheckpointNotBuilt { .. }));
    assert_eq!(state_json, "null");
}

#[tokio::test(start_paused = true)]
async fn a_state_that_panics_while_serializing_is_reported_the_same_way() {
    let (failures, _) = failed_run_with_exhausted_serialization(true).await;
    match &failures[1] {
        RunFailure::CheckpointNotBuilt { reason } => {
            assert!(reason.contains("serialization panics"));
        }
        other => panic!("unexpected failure {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_sink_step_whose_checkpoint_cannot_be_built_when_the_other_node_ends_during_the_grace_then_the_run_is_failed_not_ended()
 {
    plan(
        &[Answer::Succeed, Answer::Succeed, Answer::Fail],
        Answer::Succeed,
    );
    let gate = Arc::new(Notify::new());
    let graph = graph_of(vec![
        ScriptedNode {
            name: "a",
            behavior: Behavior::Result,
        },
        ScriptedNode {
            name: "b",
            behavior: Behavior::ResultAfter(Arc::clone(&gate)),
        },
    ]);
    let observer = Recorder::new();
    let mut config = RunConfig::new(CancelToken::new(), Duration::from_secs(10));
    config.sink = Some(Arc::new(AcceptingSink));
    config.observer = Some(Arc::clone(&observer) as _);
    let initial = State::create(Flaky(7)).unwrap();
    gate.notify_one();
    let outcome = within(start(
        graph,
        initial,
        vec!["a".to_owned(), "b".to_owned()],
        config,
    ))
    .await
    .unwrap();
    let log = within(observer.wait_for_outcome()).await;
    match outcome {
        Outcome::RunFailed {
            failures,
            checkpoint,
        } => {
            assert!(matches!(failures[0], RunFailure::CheckpointNotBuilt { .. }));
            assert_eq!(failures.len(), 1);
            assert!(checkpoint.nodes().is_empty());
        }
        other => panic!("expected RunFailed, got {other:?}"),
    }
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunFailed))
    );
}
