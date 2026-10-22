//! A runner call inside a called graph: its key names the node below the
//! calling node (`call/x`), and since a call restarts whole and records
//! nothing inside, a resumed run makes the finished runner call again under
//! that key, for the port to find.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::graph::{Always, Context, FnNode, GraphBuilder, NodeFuture, Output, SubGraph, Target};
use crate::run::control;
use crate::runner::test_support::*;
use crate::state::{Config, Kind, Schema, State};
use crate::testkit::{TestRunners, text_block};
use crate::value::EndLabel;

#[tokio::test]
async fn given_a_runner_call_in_a_called_graph_when_the_call_restarts_then_it_is_made_again_under_its_key()
 {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("an answer")]);
    // `x` asks the runner, then `y` fails once: the call fails and restarts
    // whole.
    let failures = Arc::new(AtomicUsize::new(0));
    let fails_once = failures.clone();
    let y = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let first = fails_once.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                match first {
                    true => Err("the child graph fails once".into()),
                    false => Ok(Vec::new()),
                }
            })
        },
    );
    let child_schema = Schema::builder()
        .state_by_name(key("answer"), Kind::Str)
        .build();
    let child = GraphBuilder::new(child_schema)
        .entry(nid("x"))
        .node(nid("x"), ask(&port, set_answer()))
        .node(nid("y"), y)
        .edge(nid("x"), Always(Target::Node(nid("y"))))
        .edge(
            nid("y"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .output(key("answer"))
        .build()
        .unwrap();
    let graph = GraphBuilder::new(schema())
        .entry(nid("call"))
        .subgraph(
            nid("call"),
            SubGraph::call(Arc::new(child)).output(key("answer"), Output::Set(key("answer"))),
        )
        .edge(
            nid("call"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();

    let failure = run_in(&graph, state_with("", &[]), &ctx("run-1"))
        .await
        .err()
        .unwrap();
    let (_sender, mut channel) = control();
    let state = finished(
        crate::run::run(
            &graph,
            &config(),
            failure.checkpoint.state,
            Some(failure.checkpoint.cursor),
            &ctx("run-1").with_pending(failure.checkpoint.pending),
            &mut channel,
        )
        .await,
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "an answer");
    assert_eq!(
        port.keys(),
        [
            call_key("run-1", "call/x", None, &in_french()),
            call_key("run-1", "call/x", None, &in_french())
        ],
        "nothing is recorded inside a called graph: the call is made again under its key"
    );
    assert_eq!(port.made(), 1, "the port finds the call under its key");
}
