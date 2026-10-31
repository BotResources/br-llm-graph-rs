use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_llm_clients_core::ClientError;

use crate::common::*;
use crate::kit::*;

#[tokio::test(start_paused = true)]
async fn given_a_step_when_the_client_is_called_then_the_response_is_already_open_for_the_entry_of_the_request()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("hello")]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (record, sink) = (rig.record.clone(), seen.clone());
    model.on_call(move |request| {
        sink.lock()
            .unwrap()
            .push((opened_responses(&record), request.reply_entry()));
    });
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, vec![seen[0].1]);
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_streams_when_the_step_runs_then_the_deltas_go_to_the_open_response_as_the_sink()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("hello").streaming()]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let entry = model.requests()[0].reply_entry();
    assert_eq!(opened_responses(&rig.record), vec![entry]);
    assert_eq!(deltas_given(&rig.record), vec![entry]);
}

#[tokio::test(start_paused = true)]
async fn given_a_complete_response_when_the_step_ends_then_the_open_response_is_completed_and_the_copy_holds_it_once()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("hello").streaming()]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let entry = model.requests()[0].reply_entry();
    assert_eq!(completed_responses(&rig.record), vec![entry]);
    assert_eq!(response_ids(&rig.conversation()), vec![entry]);
    assert!(position(&rig.record, "open_response") < position(&rig.record, "complete_response"));
    assert_eq!(count(&rig.record, "add_entry"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_failed_call_when_the_next_client_answers_then_the_open_response_is_discarded_and_the_next_call_has_new_entry_and_same_turn()
 {
    let rig = Rig::new();
    let first = Model::new(vec![
        Act::fail(ClientError::Unavailable { retry_after: None }).streaming(),
    ]);
    let second = Model::new(vec![Act::text("recovered")]);
    let node = node(&rig, plains(&[&first, &second]));

    let update = Driver::result(&node).await;

    let (failed, worked) = (first.requests()[0].clone(), second.requests()[0].clone());
    assert_eq!(update.text, "recovered");
    assert_ne!(failed.reply_entry(), worked.reply_entry());
    assert_eq!(failed.reply_turn(), worked.reply_turn());
    assert_eq!(
        opened_responses(&rig.record),
        vec![failed.reply_entry(), worked.reply_entry()]
    );
    assert_eq!(discarded_responses(&rig.record), vec![failed.reply_entry()]);
    assert_eq!(completed_responses(&rig.record), vec![worked.reply_entry()]);
    assert_eq!(
        response_ids(&rig.conversation()),
        vec![worked.reply_entry()]
    );
    assert_eq!(
        names(&rig.record)[..5].to_vec(),
        vec![
            "thread_port",
            "agent_port",
            "read",
            "tools_port",
            "open_response"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_failed_call_when_the_copy_is_read_then_the_failed_attempt_left_nothing_in_the_conversation()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::fail(ClientError::QuotaExceeded)]);
    let second = Model::new(vec![Act::fail(ClientError::Authentication)]);
    let node = node(&rig, plains(&[&first, &second]));

    Driver::failure(&node).await;

    assert!(response_ids(&rig.conversation()).is_empty());
    assert!(completed_responses(&rig.record).is_empty());
    assert_eq!(discarded_responses(&rig.record).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_two_steps_when_the_agent_runs_then_each_step_has_new_entry_ids_and_the_pass_keeps_one_turn_id()
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

    let requests = model.requests();
    let entries: Vec<_> = requests
        .iter()
        .map(|request| request.reply_entry())
        .collect();
    assert_eq!(requests.len(), 3);
    assert!(entries[0] != entries[1] && entries[1] != entries[2] && entries[0] != entries[2]);
    assert!(
        requests
            .iter()
            .all(|request| request.reply_turn() == requests[0].reply_turn())
    );
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].id(), requests[0].reply_turn());
    assert_eq!(response_ids(&conversation), entries);
}

#[tokio::test(start_paused = true)]
async fn given_an_open_that_takes_longer_than_the_timeout_when_the_call_is_quick_then_the_call_works()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("hello")]);
    let quirks = Quirks {
        open_response_delay: Duration::from_secs(10),
        ..Quirks::default()
    };
    let access = Wrapped::new(&rig.access, quirks);
    let node = node_over(&rig, access, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "hello");
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_step_when_the_agent_runs_then_the_node_reads_no_setting_of_the_client() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(model.settings_reads(), 0);
}
