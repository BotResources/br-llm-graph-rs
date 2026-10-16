//! Serving a session: started now or on an input, relaunched by an input
//! after an end, paused between runs, ended by a cancel.

use super::tests::linear_graph;
use super::*;
use crate::run::test_support::*;

#[tokio::test]
async fn given_serve_on_input_when_input_arrives_then_runs_then_cancel_ends() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx()).unwrap();
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
    let session = Session::new(linear_graph(), config(), base_state(), ctx()).unwrap();
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
    let session = Session::new(linear_graph(), config(), base_state(), ctx()).unwrap();
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
async fn given_paused_session_when_input_then_stays_paused_until_resume() {
    let session = Session::new(linear_graph(), config(), base_state(), ctx()).unwrap();
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
