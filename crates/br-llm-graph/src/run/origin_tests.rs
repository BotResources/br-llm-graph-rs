use std::sync::{Arc, Mutex};

use crate::graph::{Context, FnNode, Graph, GraphBuilder, Node, NodeFuture};
use crate::observe::{NoopObserver, Observer};
use crate::origin::{OccurrenceKey, Origin, RunId, Segment};
use crate::run::Cursor;
use crate::run::inbox::channel;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

#[derive(Default)]
struct Origins {
    events: Mutex<Vec<(String, Origin)>>,
}

impl Origins {
    fn push(&self, event: String, origin: &Origin) {
        self.events.lock().unwrap().push((event, origin.clone()));
    }

    fn all(&self) -> Vec<(String, Origin)> {
        self.events.lock().unwrap().clone()
    }

    fn started(&self, node: &str) -> Vec<String> {
        self.all()
            .into_iter()
            .filter(|(event, _)| *event == format!("started {node}"))
            .map(|(_, origin)| origin.occurrence.to_string())
            .collect()
    }
}

impl Observer for Origins {
    fn node_started(&self, origin: &Origin, node: &NodeId) {
        self.push(format!("started {node}"), origin);
    }
    fn node_finished(&self, origin: &Origin, node: &NodeId) {
        self.push(format!("finished {node}"), origin);
    }
    fn applied(&self, origin: &Origin, update: &Update) {
        self.push(format!("applied {update}"), origin);
    }
    fn checkpoint(&self, origin: &Origin, _state: &State, _cursor: &Cursor) {
        self.push("checkpoint".to_owned(), origin);
    }
    fn run_finished(&self, origin: &Origin, end: &EndLabel) {
        self.push(format!("ended {end}"), origin);
    }
}

fn two_steps() -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("step"))
        .node(nid("step"), log_node("x"))
        .node(nid("next"), noop_node())
        .edge(nid("step"), edge(vec![to("next")]))
        .edge(nid("next"), edge(vec![end("done")]))
        .build()
        .unwrap()
}

/// Runs a child graph to its end inside a node, on the node's own context.
struct Nested(Arc<Graph>);

impl Node for Nested {
    fn run<'a>(
        &'a self,
        _state: &'a State,
        config: &'a Config,
        ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let (_sender, mut inbox) = channel();
            let state = self.0.start_state(Vec::new())?;
            match run(&self.0, config, state, None, ctx, &mut inbox).await {
                Ok(_) => Ok(Vec::new()),
                Err(failure) => Err(failure.error.into()),
            }
        })
    }
}

#[test]
fn given_a_new_context_when_derived_then_occurrence_and_run_id_follow() {
    let ctx = Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    );
    assert!(ctx.occurrence().is_root());
    assert_eq!(ctx.run_id(), &RunId::default());
    let ctx = ctx.with_run_id(RunId::new("r1"));
    let node = ctx.for_node(&nid("a"));
    let item = node.child(Segment::item(nid("b"), 2));
    assert_eq!(item.occurrence().to_string(), "a/b[2]");
    assert_eq!(item.run_id(), &RunId::new("r1"));
    assert!(ctx.occurrence().is_root());
}

#[tokio::test]
async fn given_a_top_level_run_when_observed_then_every_event_carries_the_run_id_and_an_empty_occurrence()
 {
    let origins = Arc::new(Origins::default());
    let ctx = crate::testkit::context(origins.clone()).with_run_id(RunId::new("r1"));
    let (_sender, mut inbox) = channel();
    let outcome = run(
        &two_steps(),
        &config(),
        base_state(),
        None,
        &ctx,
        &mut inbox,
    )
    .await;
    assert!(matches!(outcome, Ok(Outcome::Finished { .. })));
    let events = origins.all();
    assert_eq!(events.len(), 8);
    let expected = Origin {
        run: RunId::new("r1"),
        occurrence: OccurrenceKey::root(),
    };
    assert!(events.iter().all(|(_, origin)| *origin == expected));
}

#[tokio::test]
async fn given_a_running_node_when_it_reads_its_context_then_it_sees_its_own_occurrence() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = seen.clone();
    let node = FnNode::new(
        move |_s: &State, _c: &Config, x: &Context| -> NodeFuture<'_> {
            sink.lock().unwrap().push(x.occurrence().to_string());
            Box::pin(async { Ok(Vec::new()) })
        },
    );
    let graph = GraphBuilder::new(schema())
        .entry(nid("probe"))
        .node(nid("probe"), node)
        .edge(nid("probe"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let ctx = crate::testkit::context(Arc::new(NoopObserver));
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), base_state(), None, &ctx, &mut inbox)
        .await
        .unwrap();
    assert_eq!(seen.lock().unwrap().clone(), vec!["probe".to_owned()]);
}

#[tokio::test]
async fn given_the_same_node_id_in_a_parent_and_a_nested_run_when_observed_then_the_origins_differ()
{
    let parent = GraphBuilder::new(schema())
        .entry(nid("step"))
        .node(nid("step"), noop_node())
        .node(nid("outer"), Nested(Arc::new(two_steps())))
        .edge(nid("step"), edge(vec![to("outer")]))
        .edge(nid("outer"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let origins = Arc::new(Origins::default());
    let ctx = crate::testkit::context(origins.clone()).with_run_id(RunId::new("r1"));
    let (_sender, mut inbox) = channel();
    run(&parent, &config(), base_state(), None, &ctx, &mut inbox)
        .await
        .unwrap();
    assert_eq!(origins.started("step"), vec!["", "outer"]);
    let ends: Vec<(String, String)> = origins
        .all()
        .into_iter()
        .filter(|(event, _)| event.starts_with("ended"))
        .map(|(event, origin)| (event, origin.occurrence.to_string()))
        .collect();
    assert_eq!(
        ends,
        vec![
            ("ended done".to_owned(), "outer".to_owned()),
            ("ended done".to_owned(), String::new())
        ]
    );
    assert!(
        origins
            .all()
            .iter()
            .all(|(_, origin)| origin.run == RunId::new("r1"))
    );
}
