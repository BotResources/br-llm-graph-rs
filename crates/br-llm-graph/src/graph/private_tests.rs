//! The private keys of a graph: its runs work on them, and no state, no
//! checkpoint and no observer checkpoint they give back holds them; a
//! checkpoint names the entry and holds no pending entry.

use std::sync::{Arc, Mutex};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, FnNode, Graph, GraphBuilder, Node, NodeFuture};
use crate::observe::Observer;
use crate::origin::Origin;
use crate::run::test_support::{edge, end, failing_node, key, nid, to};
use crate::run::{Checkpoint, Cursor, Outcome, PendingEntry, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::{SeqIds, refusal};
use crate::update::Update;

fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("log"), Kind::list(Kind::Str))
        .build()
}

/// Appends `mark` to the private key `work`, and logs how many items it
/// found there.
fn work_node(mark: &'static str) -> impl Node {
    FnNode::new(
        move |state: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            Box::pin(async move {
                let seen = state.list(&key("work"))?.len();
                Ok(vec![
                    Update::Append {
                        key: key("work"),
                        value: Value::str(mark),
                    },
                    Update::Append {
                        key: key("log"),
                        value: Value::str(format!("{mark} saw {seen}")),
                    },
                ])
            })
        },
    )
}

/// `a` then `b`, both writing the private key `work`; `b` with `sibling`
/// beside it in its superstep.
fn graph_with(b: impl Node + 'static, sibling: impl Node + 'static) -> Graph {
    GraphBuilder::new(schema())
        .private(key("work"), Kind::list(Kind::Str))
        .entry(nid("a"))
        .node(nid("a"), work_node("a"))
        .node(nid("b"), b)
        .node(nid("c"), sibling)
        .edge(nid("a"), edge(vec![to("b"), to("c")]))
        .edge(nid("b"), edge(vec![end("done")]))
        .edge(nid("c"), edge(vec![end("done")]))
        .input(key("count"))
        .output(key("count"))
        .build()
        .unwrap()
}

fn graph() -> Graph {
    graph_with(work_node("b"), work_node("c"))
}

fn start(graph: &Graph) -> State {
    graph.start_state([(key("count"), Value::int(0))]).unwrap()
}

fn config() -> Config {
    Config::new(&schema(), Default::default()).unwrap()
}

fn ctx(observer: Arc<dyn Observer>) -> Context {
    Context::new(observer, Arc::new(SeqIds::new()))
}

/// Keeps the keys of each observed checkpoint's state, its cursor, and the
/// recorded entries.
#[derive(Default)]
struct Seen {
    checkpoints: Mutex<Vec<(Vec<String>, Cursor)>>,
    recorded: Mutex<Vec<PendingEntry>>,
}

impl Observer for Seen {
    fn checkpoint(&self, _origin: &Origin, state: &State, cursor: &Cursor) {
        let keys = state.schema().state.keys().map(|k| k.to_string()).collect();
        self.checkpoints
            .lock()
            .unwrap()
            .push((keys, cursor.clone()));
    }

    fn recorded(&self, _origin: &Origin, entry: &PendingEntry) {
        self.recorded.lock().unwrap().push(entry.clone());
    }
}

fn keys_of(state: &State) -> Vec<String> {
    state.schema().state.keys().map(|k| k.to_string()).collect()
}

fn entry_cursor() -> Cursor {
    Cursor::new(vec![nid("a")], Vec::new())
}

fn log(state: &State) -> Vec<String> {
    state
        .list(&key("log"))
        .unwrap()
        .iter()
        .map(|value| value.to_string())
        .collect()
}

#[tokio::test]
async fn given_a_private_key_when_the_graph_runs_then_its_nodes_work_on_it_and_the_final_state_omits_it()
 {
    let graph = graph();
    assert!(!graph.schema().state.contains_key(&key("work")));
    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config(),
        start(&graph),
        None,
        &ctx(Arc::new(Seen::default())),
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(keys_of(&state), vec!["count", "log"]);
    assert_eq!(state.schema(), graph.schema());
    assert_eq!(
        log(&state),
        vec!["\"a saw 0\"", "\"b saw 1\"", "\"c saw 1\""]
    );
}

#[tokio::test]
async fn given_a_paused_run_when_resumed_from_its_checkpoint_then_it_omits_the_key_and_enters_again()
 {
    let graph = graph();
    let (sender, mut channel) = control();
    sender.pause();
    let outcome = run(
        &graph,
        &config(),
        start(&graph),
        None,
        &ctx(Arc::new(Seen::default())),
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();
    let Outcome::Paused { checkpoint } = outcome else {
        panic!("expected paused");
    };
    assert_eq!(keys_of(&checkpoint.state), vec!["count", "log"]);
    assert_eq!(checkpoint.cursor, entry_cursor());
    assert!(checkpoint.pending.is_empty());
    let json = serde_json::to_string(&checkpoint).unwrap();
    assert!(!json.contains("work"));
    let checkpoint: Checkpoint = serde_json::from_str(&json).unwrap();

    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx(Arc::new(Seen::default())),
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(
        log(&state),
        vec!["\"a saw 0\"", "\"a saw 0\"", "\"b saw 1\"", "\"c saw 1\""]
    );
}

#[tokio::test]
async fn given_a_node_failing_beside_a_finished_one_when_the_run_fails_then_nothing_is_recorded_and_the_checkpoint_enters_again()
 {
    let graph = graph_with(failing_node(), work_node("c"));
    let seen = Arc::new(Seen::default());
    let (_sender, mut channel) = control();
    let failure = match run(
        &graph,
        &config(),
        start(&graph),
        None,
        &ctx(seen.clone()),
        &mut channel,
    )
    .await
    {
        Err(failure) => failure,
        Ok(_) => panic!("expected the run to fail"),
    };
    assert!(matches!(failure.error, GraphError::NodeFailed { .. }));
    assert_eq!(keys_of(&failure.checkpoint.state), vec!["count", "log"]);
    assert_eq!(failure.checkpoint.cursor, entry_cursor());
    assert!(failure.checkpoint.pending.is_empty());
    assert!(seen.recorded.lock().unwrap().is_empty());
}

#[tokio::test]
async fn given_a_private_key_when_the_observer_sees_a_checkpoint_then_it_omits_the_key_and_names_the_entry()
 {
    let graph = graph();
    let seen = Arc::new(Seen::default());
    let (_sender, mut channel) = control();
    run(
        &graph,
        &config(),
        start(&graph),
        None,
        &ctx(seen.clone()),
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();
    let checkpoints = seen.checkpoints.lock().unwrap().clone();
    let keys = vec!["count".to_owned(), "log".to_owned()];
    assert_eq!(
        checkpoints,
        vec![(keys.clone(), entry_cursor()), (keys, Cursor::default())]
    );
}

#[test]
fn given_a_private_key_the_schema_declares_when_built_then_key_collision() {
    let result = GraphBuilder::new(schema())
        .private(key("log"), Kind::list(Kind::Str))
        .entry(nid("a"))
        .node(nid("a"), work_node("a"))
        .edge(nid("a"), edge(vec![end("done")]))
        .build();
    assert!(matches!(
        result,
        Err(GraphError::KeyCollision { key }) if key.as_str() == "log"
    ));
}

/// A node whose check requires the state key `work`.
struct NeedsWork;

impl Node for NeedsWork {
    fn run<'a>(&'a self, _s: &'a State, _c: &'a Config, _x: &'a Context) -> NodeFuture<'a> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        match schema.state.contains_key(&key("work")) {
            true => Ok(()),
            false => Err(GraphError::UnknownKey { key: key("work") }),
        }
    }
}

#[test]
fn given_a_node_checking_a_private_key_when_built_then_its_check_sees_the_key() {
    let build = |private: bool| {
        let builder = GraphBuilder::new(schema());
        let builder = if private {
            builder.private(key("work"), Kind::list(Kind::Str))
        } else {
            builder
        };
        builder
            .entry(nid("a"))
            .node(nid("a"), NeedsWork)
            .edge(nid("a"), edge(vec![end("done")]))
            .build()
    };
    assert!(build(true).is_ok());
    assert!(matches!(
        refusal(build(false)),
        GraphError::UnknownKey { .. }
    ));
}
