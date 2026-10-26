use std::sync::Arc;

use br_graph_contract::{Cause, Edge, Graph, GraphBuilder, Node, State, Target};

use crate::kv::Kv;
use crate::nodes::{Stage, TestError, TestNode};

type Behavior = dyn Fn(&State<Kv>) -> Result<Vec<Target>, Cause> + Send + Sync;

pub struct ScriptedEdge {
    name: String,
    source: String,
    declared: Vec<Target>,
    behavior: Box<Behavior>,
}

impl Edge<Kv> for ScriptedEdge {
    fn name(&self) -> &str {
        &self.name
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn targets(&self) -> &[Target] {
        &self.declared
    }

    fn evaluate(&self, state: &State<Kv>) -> Result<Vec<Target>, Cause> {
        (self.behavior)(state)
    }
}

pub fn targets(names: &[&str]) -> Vec<Target> {
    names.iter().map(|name| Target::from(*name)).collect()
}

pub fn edge_answering(
    source: &str,
    declared: &[&str],
    behavior: impl Fn(&State<Kv>) -> Result<Vec<Target>, Cause> + Send + Sync + 'static,
) -> Arc<dyn Edge<Kv>> {
    Arc::new(ScriptedEdge {
        name: format!("after_{source}"),
        source: source.to_owned(),
        declared: targets(declared),
        behavior: Box::new(behavior),
    })
}

pub fn named_edge(name: &str, source: &str, declared: &[&str]) -> Arc<dyn Edge<Kv>> {
    let answer = targets(declared);
    Arc::new(ScriptedEdge {
        name: name.to_owned(),
        source: source.to_owned(),
        declared: answer.clone(),
        behavior: Box::new(move |_| Ok(answer.clone())),
    })
}

pub fn edge(source: &str, declared: &[&str]) -> Arc<dyn Edge<Kv>> {
    let answer = targets(declared);
    edge_answering(source, declared, move |_| Ok(answer.clone()))
}

pub fn edge_returning(source: &str, declared: &[&str], answer: &[&str]) -> Arc<dyn Edge<Kv>> {
    let answer = targets(answer);
    edge_answering(source, declared, move |_| Ok(answer.clone()))
}

pub fn edge_failing(source: &str, declared: &[&str], message: &'static str) -> Arc<dyn Edge<Kv>> {
    edge_answering(source, declared, move |_| {
        Err(Cause::from(TestError(message.to_owned())))
    })
}

pub fn edge_panicking(source: &str, declared: &[&str]) -> Arc<dyn Edge<Kv>> {
    edge_answering(source, declared, |_| panic!("the edge panics"))
}

pub fn routing_on(
    source: &str,
    declared: &[&str],
    route: impl Fn(&Kv) -> Vec<&'static str> + Send + Sync + 'static,
) -> Arc<dyn Edge<Kv>> {
    edge_answering(source, declared, move |state| {
        Ok(targets(&route(state.fields())))
    })
}

pub struct Rig {
    pub graph: Arc<Graph<Kv>>,
    pub stage: Arc<Stage>,
}

pub struct RigBuilder {
    stage: Arc<Stage>,
    builder: GraphBuilder<Kv>,
}

pub fn rig(version: &str) -> RigBuilder {
    RigBuilder {
        stage: Stage::new(),
        builder: GraphBuilder::new(version),
    }
}

impl RigBuilder {
    pub fn node(self, name: &str) -> Self {
        self.node_of(name, "programmatic", "worker")
    }

    pub fn node_of(mut self, name: &str, kind: &str, subkind: &str) -> Self {
        let node: Arc<dyn Node<Kv>> = Arc::new(TestNode {
            name: name.to_owned(),
            kind: kind.to_owned(),
            subkind: subkind.to_owned(),
            stage: Arc::clone(&self.stage),
        });
        self.builder = self.builder.node(node);
        self
    }

    pub fn edge(mut self, edge: Arc<dyn Edge<Kv>>) -> Self {
        self.builder = self.builder.edge(edge);
        self
    }

    pub fn entry(mut self, name: &str) -> Self {
        self.builder = self.builder.entry(name);
        self
    }

    pub fn build(self) -> Rig {
        Rig {
            graph: Arc::new(self.builder.build().unwrap()),
            stage: self.stage,
        }
    }
}

pub fn chain(version: &str, names: &[&str]) -> Rig {
    let mut builder = rig(version).entry(names[0]);
    for (position, name) in names.iter().enumerate() {
        let next = names.get(position + 1).copied().unwrap_or("END");
        builder = builder.node(name).edge(edge(name, &[next]));
    }
    builder.build()
}

pub fn single(name: &str) -> Rig {
    chain("1", &[name])
}

pub fn fan(source: &str, branches: &[&str]) -> Rig {
    let mut builder = rig("1")
        .entry(source)
        .node(source)
        .edge(edge(source, branches));
    for branch in branches {
        builder = builder.node(branch).edge(edge(branch, &["END"]));
    }
    builder.build()
}
