//! A runner call node a loop of its graph runs again: with the same
//! arguments it names the same call and gets the same answer; with other
//! arguments it makes another call.

use std::sync::Arc;

use serde_json::{Map, Value as Json};

use crate::graph::{Always, Context, FnEdge, FnNode, Graph, GraphBuilder, NodeFuture, Target};
use crate::runner::test_support::*;
use crate::runner::{RunnerArguments, RunnerCall};
use crate::state::{Config, State, StateKey};
use crate::testkit::{Scripted, TestRunners, text_block};
use crate::value::EndLabel;

/// `ask`, then `count` adds one to `count`, back to `ask` until it is 2.
fn looping(ask: RunnerCall) -> Graph {
    let count: StateKey<i64> = skey("count");
    let (read, write) = (count.clone(), count.clone());
    let add_one = FnNode::new(
        move |state: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
            let update = state.get(&read).and_then(|n| write.set(n + 1));
            Box::pin(async move { Ok(vec![update?]) })
        },
    );
    let done = Target::End(EndLabel::new("done").unwrap());
    let end = done.clone();
    let again = FnEdge::new(move |state, _| {
        Ok(vec![match state.get(&count)? < 2 {
            true => Target::Node(nid("ask")),
            false => end.clone(),
        }])
    })
    .targets([Target::Node(nid("ask")), done]);
    GraphBuilder::new(schema())
        .entry(nid("ask"))
        .node(nid("ask"), ask)
        .node(nid("count"), add_one)
        .edge(nid("ask"), Always(Target::Node(nid("count"))))
        .edge(nid("count"), again)
        .build()
        .unwrap()
}

/// A port whose `documentalist` answers with the round it is asked about,
/// or `the same` when it is asked about none.
fn rounds() -> Arc<TestRunners> {
    let port = Arc::new(TestRunners::new());
    port.script("documentalist", |request| {
        let text = match request.arguments.get("round") {
            Some(round) => format!("round {round}"),
            None => "the same".to_owned(),
        };
        Scripted::Answer(vec![text_block(&text)])
    });
    port
}

#[tokio::test]
async fn given_a_loop_that_runs_a_runner_call_again_with_the_same_arguments_when_run_then_one_call_and_the_same_answer()
 {
    let port = rounds();
    let state = finished(
        run_in(
            &looping(ask(&port, append_answers())),
            state_with("", &[]),
            &ctx("run-1"),
        )
        .await,
    );

    assert_eq!(texts(&state, "answers"), ["the same", "the same"]);
    assert_eq!(
        port.keys(),
        [
            call_key("run-1", "ask", None, &in_french()),
            call_key("run-1", "ask", None, &in_french())
        ]
    );
    assert_eq!(port.made(), 1, "the same key names the same call");
}

#[tokio::test]
async fn given_a_loop_that_runs_a_runner_call_again_with_other_arguments_when_run_then_another_call()
 {
    let port = rounds();
    let count: StateKey<i64> = skey("count");
    let by_round = RunnerCall {
        arguments: RunnerArguments::from_fn(move |state, _| {
            Ok(Map::from_iter([(
                "round".to_owned(),
                Json::from(state.get(&count)?),
            )]))
        }),
        ..ask(&port, append_answers())
    };
    let state = finished(run_in(&looping(by_round), state_with("", &[]), &ctx("run-1")).await);

    assert_eq!(texts(&state, "answers"), ["round 0", "round 1"]);
    let keys = port.keys();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1]);
    assert_eq!(port.made(), 2);
}
