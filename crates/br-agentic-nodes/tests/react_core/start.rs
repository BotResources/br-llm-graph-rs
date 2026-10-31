use std::sync::Arc;

use br_agentic_contract::{FileSystemMode, WorkspaceFile};
use br_agentic_nodes::AuthorSource;
use br_llm_messages::{ClosureReason, IncomingEntry, InputTurn, ResponseEnding, Turn, TurnEntry};

use crate::common::*;
use crate::kit::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

fn interrupted(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::Interrupted { .. }))
}

#[tokio::test(start_paused = true)]
async fn given_a_new_instance_when_it_starts_then_each_start_port_is_called_once_before_the_first_step()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("done"),
    ]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    for name in ["thread_port", "agent_port", "read", "tools_port"] {
        assert_eq!(count(&rig.record, name), 1, "{name}");
    }
    assert_eq!(agent_labels(&rig.record), vec!["agent"]);
    assert_eq!(rig.prompts.asked("agent"), 1);
    assert_eq!(rig.prompts.asked("generator"), 0);
    let first_step = position(&rig.record, "open_response");
    for name in ["thread_port", "agent_port", "read", "tools_port"] {
        assert!(position(&rig.record, name) < first_step, "{name}");
    }
    assert!(position(&rig.record, "thread_port") < position(&rig.record, "agent_port"));
    assert!(position(&rig.record, "agent_port") < position(&rig.record, "read"));
}

#[tokio::test(start_paused = true)]
async fn given_two_inputs_when_the_instance_starts_then_every_start_port_gets_the_snapshot_of_the_first_input()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("done")]);
    let prompt = Arc::new(SnapPrompt::default());
    let author = SnapAuthor::new("Ada");
    let tools = SnapTools::new(&rig.record, Vec::new());
    let capture = Capture::new(&rig.record);
    let node = node_with_ports(
        &rig,
        prompt.clone(),
        tools.clone(),
        AuthorSource::Port(author.clone()),
        capture.clone(),
        plains(&[&model]),
    );
    let mut driver = Driver::start(&node).await;

    give_note(&driver, "first");
    give_note(&driver, "second");
    driver.event().await;

    let first = "first".to_owned();
    assert_eq!(
        prompt.seen.lock().unwrap().clone(),
        vec![(first.clone(), "agent".to_owned())]
    );
    assert_eq!(
        author.seen.lock().unwrap().clone(),
        vec![(first.clone(), "agent".to_owned())]
    );
    assert_eq!(tools.seen.lock().unwrap().clone(), vec![first.clone()]);
    assert_eq!(capture.seen.lock().unwrap()[0].note, first);
}

#[tokio::test(start_paused = true)]
async fn given_a_first_call_that_fails_when_the_client_runs_then_no_ai_turn_exists_before_the_first_response()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::fail(br_llm_clients_core::ClientError::Transport)]);
    let second = Model::new(vec![Act::text("fine")]);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    for model in [&first, &second] {
        let (access, sink) = (rig.access.clone(), seen.clone());
        model.on_call(move |_| {
            sink.lock()
                .unwrap()
                .push(ai_turns(&access.conversation()).len());
        });
    }
    let node = node(&rig, plains(&[&first, &second]));

    Driver::result(&node).await;

    assert_eq!(seen.lock().unwrap().clone(), vec![0, 0]);
    assert_eq!(ai_turns(&rig.conversation()).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_first_call_that_fails_for_good_when_the_instance_fails_then_no_ai_turn_was_created()
 {
    let rig = Rig::new();
    let only = Model::new(vec![Act::fail(br_llm_clients_core::ClientError::Transport)]);
    let node = node(&rig, plains(&[&only]));

    Driver::failure(&node).await;

    assert!(ai_turns(&rig.conversation()).is_empty());
}
