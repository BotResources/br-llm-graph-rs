use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use br_graph_contract::{
    CancelView, Cause, CreateError, CreateFuture, Edge, Event, Fields, Graph, GraphBuilder,
    Instance, InstanceId, Node, PROGRAMMATIC, Reason, State, Target,
};
use br_graph_executor::{Checkpoint, CheckpointSink, SinkFuture};
use tokio::sync::Notify;

use crate::host::within;
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const PANICS_WHEN_DESERIALIZED: u32 = 13;

#[derive(Clone, Copy)]
pub enum Answer {
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

pub fn plan(script: &[Answer], then: Answer) {
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
pub struct Flaky(pub u32);

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
        let value = Option::<u32>::deserialize(deserializer).map_err(D::Error::custom)?;
        if value == Some(PANICS_WHEN_DESERIALIZED) {
            panic!("deserialization panics");
        }
        Ok(Flaky(value.unwrap_or_default()))
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

pub enum Behavior {
    Fail,
    Result,
    ResultAfter(Arc<Notify>),
    Silent,
}

pub struct ScriptedNode {
    pub name: &'static str,
    pub behavior: Behavior,
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
        let silent = matches!(self.behavior, Behavior::Silent);
        Box::pin(async move {
            let (instance, mut reader, sender) = Instance::<Flaky>::channel(id);
            tokio::spawn(async move {
                let _ = reader.next().await;
                if silent {
                    std::future::pending::<()>().await;
                }
                if let Some(gate) = release {
                    within(gate.notified()).await;
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

pub struct AcceptingSink;

impl CheckpointSink for AcceptingSink {
    fn accept<'a>(&'a self, _checkpoint: &'a Checkpoint) -> SinkFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

pub fn graph_of(nodes: Vec<ScriptedNode>) -> Arc<Graph<Flaky>> {
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
