use std::sync::Arc;

use br_graph_contract::{
    BuildError, CancelView, CreateError, CreateFuture, Edge, Event, Fields, GraphBuilder, Instance,
    InstanceId, Node, PROGRAMMATIC, Reason, State, Target,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tally {
    pub total: i64,
    pub notes: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum TallyUpdate {
    Nothing,
    Add(i64),
    Note(String),
    Reject,
}

impl Fields for Tally {
    type Update = TallyUpdate;

    fn empty_update() -> TallyUpdate {
        TallyUpdate::Nothing
    }

    fn validate(&self) -> Result<(), Reason> {
        if self.total < 0 {
            return Err(Reason::new("total is negative"));
        }
        Ok(())
    }

    fn apply(&mut self, update: TallyUpdate) -> Result<(), Reason> {
        match update {
            TallyUpdate::Nothing => Ok(()),
            TallyUpdate::Add(amount) => {
                self.total += amount;
                Ok(())
            }
            TallyUpdate::Note(note) => {
                self.notes.push(note);
                Ok(())
            }
            TallyUpdate::Reject => Err(Reason::new("the update is rejected")),
        }
    }
}

pub fn tally(total: i64) -> Tally {
    Tally {
        total,
        notes: Vec::new(),
    }
}

pub fn state(total: i64) -> State<Tally> {
    State::create(tally(total)).unwrap()
}

pub fn id(seed: u128) -> InstanceId {
    let version_and_variant = (0x7_u128 << 76) | (0b10_u128 << 62);
    InstanceId::new(Uuid::from_u128(version_and_variant | seed)).unwrap()
}

pub struct PlainNode {
    pub name: String,
    pub kind: String,
    pub subkind: String,
}

pub fn plain(name: &str) -> Arc<dyn Node<Tally>> {
    Arc::new(PlainNode {
        name: name.into(),
        kind: PROGRAMMATIC.into(),
        subkind: "worker".into(),
    })
}

pub fn custom_node(name: &str, kind: &str, subkind: &str) -> Arc<dyn Node<Tally>> {
    Arc::new(PlainNode {
        name: name.into(),
        kind: kind.into(),
        subkind: subkind.into(),
    })
}

impl Node<Tally> for PlainNode {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn subkind(&self) -> &str {
        &self.subkind
    }

    fn create(&self, _cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Tally> {
        Box::pin(async move {
            let (instance, mut reader, sender) = Instance::<Tally>::channel(id);
            tokio::spawn(async move {
                let event = match reader.next().await {
                    Some(input) => Event::Result(TallyUpdate::Add(input.fields().total)),
                    None => Event::Cancelled,
                };
                sender.finish(event);
            });
            Ok::<_, CreateError>(instance)
        })
    }
}

pub struct FixedEdge {
    pub name: String,
    pub source: String,
    pub declared: Vec<Target>,
    pub answer: Result<Vec<Target>, String>,
}

impl Edge<Tally> for FixedEdge {
    fn name(&self) -> &str {
        &self.name
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn targets(&self) -> &[Target] {
        &self.declared
    }

    fn evaluate(&self, _state: &State<Tally>) -> Result<Vec<Target>, br_graph_contract::Cause> {
        self.answer.clone().map_err(Into::into)
    }
}

pub fn targets(names: &[&str]) -> Vec<Target> {
    names.iter().map(|name| Target::from(*name)).collect()
}

pub fn edge(source: &str, declared: &[&str]) -> Arc<dyn Edge<Tally>> {
    edge_answering(source, declared, Ok(targets(declared)))
}

pub fn edge_answering(
    source: &str,
    declared: &[&str],
    answer: Result<Vec<Target>, String>,
) -> Arc<dyn Edge<Tally>> {
    Arc::new(FixedEdge {
        name: format!("after_{source}"),
        source: source.into(),
        declared: targets(declared),
        answer,
    })
}

pub fn chain() -> GraphBuilder<Tally> {
    GraphBuilder::new("1")
        .node(plain("first"))
        .node(plain("second"))
        .node(plain("third"))
        .entry("first")
        .edge(edge("first", &["second"]))
        .edge(edge("second", &["third"]))
        .edge(edge("third", &["END"]))
}

pub fn fixed_edge(name: &str, source: &str, declared: &[&str]) -> Arc<dyn Edge<Tally>> {
    Arc::new(FixedEdge {
        name: name.into(),
        source: source.into(),
        declared: targets(declared),
        answer: Ok(targets(declared)),
    })
}

pub fn single(node: Arc<dyn Node<Tally>>, name: &str) -> GraphBuilder<Tally> {
    GraphBuilder::new("1")
        .node(node)
        .entry(name)
        .edge(fixed_edge("only_edge", name, &["END"]))
}

pub fn refused(builder: GraphBuilder<Tally>) -> BuildError {
    builder.build().unwrap_err()
}
