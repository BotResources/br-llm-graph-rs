use std::error::Error;
use std::sync::{Arc, Mutex};

use super::test_support::*;
use crate::error::{GraphError, NodeFault};
use crate::graph::subgraph::{Input, Output, SubGraph};
use crate::graph::{Always, Context, FnEdge, FnNode, Graph, GraphBuilder, NodeFuture, Target};
use crate::observe::{NoopObserver, Observer};
use crate::origin::Origin;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::NodeId;

#[tokio::test]
async fn given_private_keys_named_like_the_callers_when_called_then_the_callers_keys_are_untouched()
{
    let call = refine_call().output(key("count"), Output::Set(key("steps")));
    let graph = parent_with(call).unwrap();
    let state = finished(&graph, "q", 3).await;
    assert_eq!(state.str(&key("result")).unwrap(), "q#3");
    assert_eq!(state.int(&key("steps")).unwrap(), 3);
    assert_eq!(state.int(&key("count")).unwrap(), 100);
    assert!(state.bool(&key("flag")).unwrap());
}

#[tokio::test]
async fn given_end_label_output_when_called_then_the_label_is_written_as_a_string() {
    let call = refine_call().output_end_label(Output::Set(key("label")));
    let state = finished(&parent_with(call).unwrap(), "q", 1).await;
    assert_eq!(state.str(&key("label")).unwrap(), "approved");
}

#[tokio::test]
async fn given_const_and_config_sources_when_called_then_the_child_reads_them() {
    let call = SubGraph::call(refine_graph())
        .input(key("text"), Input::Const(Value::str("fixed")))
        .config(key("rounds"), Input::Const(Value::int(2)))
        .output(key("answer"), Output::Set(key("result")));
    let state = finished(&parent_with(call).unwrap(), "ignored", 9).await;
    assert_eq!(state.str(&key("result")).unwrap(), "fixed#2");
}

/// Calls the loop until `results` holds two answers, appending each answer.
fn twice_caller() -> Graph {
    let call = SubGraph::call(refine_graph())
        .input(key("text"), Input::From(key("question")))
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Append(key("results")))
        .output_end_label(Output::Append(key("labels")));
    let again = FnEdge::new(|s: &State, _c: &Config| {
        if s.list(&key("results"))?.len() < 2 {
            Ok(vec![Target::Node(nid("ask"))])
        } else {
            Ok(vec![end("done")])
        }
    });
    GraphBuilder::new(parent_schema())
        .entry(nid("ask"))
        .subgraph(nid("ask"), call)
        .edge(nid("ask"), again)
        .input(key("question"))
        .input(key("count"))
        .input(key("flag"))
        .build()
        .unwrap()
}

#[tokio::test]
async fn given_two_calls_appending_when_run_then_both_answers_appended_and_the_child_kept_no_memory()
 {
    let state = finished(&twice_caller(), "q", 2).await;
    assert_eq!(
        state.list(&key("results")).unwrap(),
        &[Value::str("q#2"), Value::str("q#2")]
    );
    assert_eq!(
        state.list(&key("labels")).unwrap(),
        &[Value::str("approved"), Value::str("approved")]
    );
}

/// A wrapper: `prepare` upper-cases `text` into `prepared`, then `inner`
/// calls the refinement loop on it. Input `text`, output `answer`.
fn wrapper_graph() -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("text"), Kind::Str)
        .state(key("prepared"), Kind::Str)
        .state(key("answer"), Kind::Str)
        .state(key("count"), Kind::Int)
        .config(key("rounds"), Kind::Int)
        .build();
    let prepare = FnNode::new(|s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        let prepared = s.str(&key("text")).map(str::to_uppercase);
        Box::pin(async move {
            Ok(vec![Update::Set {
                key: key("prepared"),
                value: Value::str(prepared?),
            }])
        })
    });
    let inner = SubGraph::call(refine_graph())
        .input(key("text"), Input::From(key("prepared")))
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Set(key("answer")));
    let graph = GraphBuilder::new(schema)
        .entry(nid("prepare"))
        .node(nid("prepare"), prepare)
        .subgraph(nid("inner"), inner)
        .edge(nid("prepare"), Always(Target::Node(nid("inner"))))
        .edge(nid("inner"), Always(end("done")))
        .input(key("text"))
        .output(key("answer"))
        .build()
        .unwrap();
    Arc::new(graph)
}

fn wrapper_call() -> SubGraph {
    SubGraph::call(wrapper_graph())
        .input(key("text"), Input::From(key("question")))
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Set(key("result")))
}

#[tokio::test]
async fn given_a_wrapper_that_calls_another_graph_when_called_then_two_levels_run() {
    let state = finished(&parent_with(wrapper_call()).unwrap(), "q", 2).await;
    assert_eq!(state.str(&key("result")).unwrap(), "Q#2");
    assert_eq!(state.int(&key("count")).unwrap(), 100);
}

#[derive(Default)]
struct Starts(Mutex<Vec<String>>);

impl Observer for Starts {
    fn node_started(&self, origin: &Origin, node: &NodeId) {
        let line = format!("{}|{node}", origin.occurrence);
        self.0.lock().unwrap().push(line);
    }
}

#[tokio::test]
async fn given_nested_calls_when_observed_then_each_level_is_prefixed_by_its_calling_node() {
    let graph = parent_with(wrapper_call()).unwrap();
    let starts = Arc::new(Starts::default());
    let state = parent_state(&graph, "q");
    run_parent(&graph, state, &parent_config(2), starts.clone())
        .await
        .unwrap();
    assert_eq!(
        starts.0.lock().unwrap().clone(),
        vec![
            "|ask",
            "ask|prepare",
            "ask|inner",
            "ask/inner|draft",
            "ask/inner|draft"
        ]
    );
}

#[derive(Debug)]
struct Broken;

impl std::fmt::Display for Broken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("broken on purpose")
    }
}

impl Error for Broken {}

fn failing_child() -> Arc<Graph> {
    let schema = Schema::builder().state(key("answer"), Kind::Str).build();
    let fail = FnNode::new(|_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async { Err(Box::new(Broken) as crate::graph::NodeError) })
    });
    let graph = GraphBuilder::new(schema)
        .entry(nid("work"))
        .node(nid("work"), fail)
        .edge(nid("work"), Always(end("done")))
        .output(key("answer"))
        .build()
        .unwrap();
    Arc::new(graph)
}

#[tokio::test]
async fn given_a_failing_child_when_called_then_the_failure_propagates_with_its_cause() {
    let call = SubGraph::call(failing_child()).output(key("answer"), Output::Set(key("result")));
    let graph = parent_with(call).unwrap();
    let state = parent_state(&graph, "q");
    let failure = run_parent(&graph, state, &parent_config(1), Arc::new(NoopObserver))
        .await
        .err()
        .unwrap();
    let GraphError::NodeFailed {
        node,
        source: NodeFault::Returned(returned),
    } = &failure.error
    else {
        panic!("expected the calling node to fail");
    };
    assert_eq!(node, &nid("ask"));
    let Some(GraphError::SubGraphFailed { source }) = returned.downcast_ref::<GraphError>() else {
        panic!("expected a sub-graph failure");
    };
    assert!(matches!(
        source.as_ref(),
        GraphError::NodeFailed { node, .. } if node == &nid("work")
    ));
    let mut cause: Option<&(dyn Error + 'static)> = Some(&failure.error);
    let mut found = false;
    while let Some(error) = cause {
        found |= error.downcast_ref::<Broken>().is_some();
        cause = error.source();
    }
    assert!(found, "the child's own error is not reachable");
    assert_eq!(failure.checkpoint.state.str(&key("result")).unwrap(), "");
}
