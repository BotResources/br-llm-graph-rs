use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_messages::{Text, UserBlock, UserInput, UserSource};

use super::*;
use crate::graph::{Always, FnNode, Graph, GraphBuilder, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::Outcome;
use crate::run::test_support::*;
use crate::state::Value;
use crate::testkit::SeqIds;
use crate::update::Update;

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

fn linear_graph() -> Arc<Graph> {
    Arc::new(
        GraphBuilder::new(schema())
            .entry(nid("a"))
            .node(nid("a"), set_count_node(5))
            .node(nid("b"), noop_node())
            .edge(nid("a"), Always(to("b")))
            .edge(nid("b"), Always(end("done")))
            .build()
            .unwrap(),
    )
}

fn input(body: &str) -> UserInput {
    UserInput::new(
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

#[tokio::test]
async fn given_new_session_when_run_once_twice_then_relaunches_from_entry() {
    let mut session = Session::new(linear_graph(), config(), base_state(), ctx());
    let first = session.run_once().await.unwrap();
    assert!(matches!(first, Outcome::Finished { .. }));
    assert_eq!(session.state().int(&key("count")).unwrap(), 5);
    let second = session.run_once().await.unwrap();
    assert!(matches!(second, Outcome::Finished { .. }));
}

#[tokio::test]
async fn given_pause_checkpoint_when_resumed_then_finishes() {
    let mut session = Session::new(linear_graph(), config(), base_state(), ctx());
    session.sender().pause();
    let paused = session.run_once().await.unwrap();
    let Outcome::Paused { checkpoint } = paused else {
        panic!("expected paused");
    };
    let json = serde_json::to_string(&checkpoint).unwrap();
    let restored: crate::run::Checkpoint = serde_json::from_str(&json).unwrap();
    let mut resumed = Session::resume(linear_graph(), config(), restored, ctx()).unwrap();
    let outcome = resumed.run_once().await.unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
    assert_eq!(resumed.state().int(&key("count")).unwrap(), 5);
}

#[tokio::test]
async fn given_empty_cursor_checkpoint_when_resumed_then_behaves_like_new() {
    let checkpoint = crate::run::Checkpoint::new(base_state(), crate::run::Cursor::default());
    let mut session = Session::resume(linear_graph(), config(), checkpoint, ctx()).unwrap();
    let outcome = session.run_once().await.unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
    assert_eq!(session.state().int(&key("count")).unwrap(), 5);
}

#[tokio::test]
async fn given_unknown_node_checkpoint_when_resumed_then_refused() {
    let checkpoint = crate::run::Checkpoint::new(
        base_state(),
        crate::run::Cursor::new(vec![nid("ghost")], Vec::new()),
    );
    assert!(Session::resume(linear_graph(), config(), checkpoint, ctx()).is_err());
}

#[tokio::test]
async fn given_serve_on_input_when_input_arrives_then_runs_then_cancel_ends() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx());
    let sender = session.sender();
    let handle = tokio::spawn(session.serve(Start::OnInput));
    sender.send(key("chat"), input("go"));
    sender.cancel();
    let ended = handle.await.unwrap();
    let Ended::Cancelled { checkpoint } = ended else {
        panic!("expected cancelled");
    };
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 0);
    assert_eq!(
        checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
}

#[tokio::test]
async fn given_serve_now_when_started_then_runs_then_relaunches_after_end() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx());
    let sender = session.sender();
    let handle = tokio::spawn(session.serve(Start::Now));
    tokio::task::yield_now().await;
    sender.send(key("chat"), input("again"));
    tokio::task::yield_now().await;
    sender.cancel();
    let ended = handle.await.unwrap();
    let Ended::Cancelled { checkpoint } = ended else {
        panic!("expected cancelled");
    };
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 5);
    assert_eq!(
        checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
}

#[tokio::test]
async fn given_serve_on_input_when_second_input_after_end_then_relaunches() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx());
    let sender = session.sender();
    let handle = tokio::spawn(session.serve(Start::OnInput));
    sender.send(key("chat"), input("first"));
    tokio::task::yield_now().await;
    sender.send(key("chat"), input("second"));
    tokio::task::yield_now().await;
    sender.cancel();
    let ended = handle.await.unwrap();
    let Ended::Cancelled { checkpoint } = ended else {
        panic!("expected cancelled");
    };
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 5);
    assert_eq!(
        checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        2
    );
}

#[tokio::test]
async fn given_cancel_checkpoint_when_resumed_then_finishes() {
    let mut session = Session::new(linear_graph(), config(), base_state(), ctx());
    session.sender().cancel();
    let cancelled = session.run_once().await.unwrap();
    let Outcome::Cancelled { checkpoint } = cancelled else {
        panic!("expected cancelled");
    };
    let mut resumed = Session::resume(linear_graph(), config(), checkpoint, ctx()).unwrap();
    let outcome = resumed.run_once().await.unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
    assert_eq!(resumed.state().int(&key("count")).unwrap(), 5);
}

#[tokio::test]
async fn given_run_failure_checkpoint_when_resumed_then_finishes_on_retry() {
    let flag = Arc::new(AtomicUsize::new(0));
    let node_flag = flag.clone();
    let graph = Arc::new(
        GraphBuilder::new(schema())
            .entry(nid("a"))
            .node(
                nid("a"),
                FnNode::new(move |_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
                    let flag = node_flag.clone();
                    Box::pin(async move {
                        if flag.fetch_add(1, Ordering::SeqCst) == 0 {
                            Err("first attempt fails".into())
                        } else {
                            Ok(vec![Update::Set {
                                key: key("count"),
                                value: Value::int(9),
                            }])
                        }
                    })
                }),
            )
            .edge(nid("a"), Always(end("done")))
            .build()
            .unwrap(),
    );
    let mut session = Session::new(graph.clone(), config(), base_state(), ctx());
    let error = session.run_once().await.err().unwrap();
    assert!(matches!(error, crate::error::GraphError::NodeFailed { .. }));
    let checkpoint = session.checkpoint();
    let mut resumed = Session::resume(graph, config(), checkpoint, ctx()).unwrap();
    let outcome = resumed.run_once().await.unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
    assert_eq!(resumed.state().int(&key("count")).unwrap(), 9);
}

#[tokio::test]
async fn given_inbox_input_on_non_conversation_key_when_served_then_failed() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx());
    let sender = session.sender();
    sender.send(key("count"), input("oops"));
    let ended = session.serve(Start::OnInput).await;
    let Ended::Failed { error, .. } = ended else {
        panic!("expected failed");
    };
    assert!(matches!(
        error,
        crate::error::GraphError::NotConversation { .. }
    ));
}

#[tokio::test]
async fn given_paused_session_when_input_then_stays_paused_until_resume() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx());
    let sender = session.sender();
    let handle = tokio::spawn(session.serve(Start::OnInput));
    sender.pause();
    sender.send(key("chat"), input("while paused"));
    sender.resume();
    sender.cancel();
    let ended = handle.await.unwrap();
    let Ended::Cancelled { checkpoint } = ended else {
        panic!("expected cancelled");
    };
    assert_eq!(
        checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 0);
}
