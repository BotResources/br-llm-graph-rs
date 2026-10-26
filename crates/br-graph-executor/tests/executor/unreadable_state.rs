use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{
    CancelToken, CancelView, Cause, CreateError, CreateFuture, Edge, Event, Fields, GraphBuilder,
    Instance, InstanceId, Node, PROGRAMMATIC, Reason, State, Target,
};
use br_graph_executor::{Outcome, RunConfig, RunFailure, start};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

thread_local! {
    static SERIALIZATIONS_LEFT: Cell<usize> = const { Cell::new(0) };
    static PANIC_WHEN_EXHAUSTED: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug, Clone, PartialEq)]
struct Flaky(u32);

impl Serialize for Flaky {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let allowed = SERIALIZATIONS_LEFT.with(|left| {
            let remaining = left.get();
            left.set(remaining.saturating_sub(1));
            remaining > 0
        });
        if allowed {
            serializer.serialize_u32(self.0)
        } else if PANIC_WHEN_EXHAUSTED.with(Cell::get) {
            panic!("serialization panics")
        } else {
            Err(S::Error::custom("serialization exhausted"))
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

struct FailingNode;

impl Node<Flaky> for FailingNode {
    fn name(&self) -> &str {
        "a"
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "worker"
    }

    fn create(&self, _cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Flaky> {
        Box::pin(async move {
            let (instance, mut reader, sender) = Instance::<Flaky>::channel(id);
            tokio::spawn(async move {
                let _ = reader.next().await;
                sender.finish(Event::failed("boom"));
            });
            Ok::<_, CreateError>(instance)
        })
    }
}

struct EndEdge;

impl Edge<Flaky> for EndEdge {
    fn name(&self) -> &str {
        "after_a"
    }

    fn source(&self) -> &str {
        "a"
    }

    fn targets(&self) -> &[Target] {
        &[Target::End]
    }

    fn evaluate(&self, _state: &State<Flaky>) -> Result<Vec<Target>, Cause> {
        Ok(vec![Target::End])
    }
}

async fn failed_run_with_exhausted_serialization(panics: bool) -> (Vec<RunFailure>, String) {
    SERIALIZATIONS_LEFT.with(|left| left.set(1));
    PANIC_WHEN_EXHAUSTED.with(|flag| flag.set(panics));
    let graph = GraphBuilder::<Flaky>::new("1")
        .node(Arc::new(FailingNode))
        .edge(Arc::new(EndEdge))
        .entry("a")
        .build()
        .unwrap();
    let initial = State::create(Flaky(7)).unwrap();
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let outcome = start(Arc::new(graph), initial, vec!["a".to_owned()], config)
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
