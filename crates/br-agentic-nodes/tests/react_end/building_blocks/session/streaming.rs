use super::*;

#[tokio::test]
async fn given_an_open_response_when_it_is_completed_then_the_whole_response_is_stored_and_no_object_stays_open()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let _stream = session.open_response(&turn, entry).await.unwrap();

    session
        .complete_response(&turn, response_text(entry, "whole"))
        .await
        .unwrap();

    assert!(session.holds(entry));
    assert_eq!(session.open_objects(), 0);
    assert!(world.access.conversation().entry(&entry).is_ok());
}

#[tokio::test]
async fn given_an_open_response_that_the_rules_refuse_when_it_is_completed_then_the_open_object_is_discarded_and_the_refusal_is_a_fault()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();
    session
        .add_entry(&turn, TurnEntry::from(response_text(entry, "taken")))
        .await
        .unwrap();
    let _stream = session.open_response(&turn, entry).await.unwrap();

    let error = session
        .complete_response(&turn, response_text(entry, "again"))
        .await
        .unwrap_err();

    assert!(matches!(error, SessionError::Rules(_)));
    assert_eq!(session.open_objects(), 0);
    assert!(
        world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, Call::DiscardResponse { entry: e, .. } if *e == entry))
    );
}

#[tokio::test]
async fn given_an_object_that_was_not_opened_when_the_session_completes_it_then_it_is_stored() {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();

    session
        .complete_response(&turn, response_text(entry, "no stream"))
        .await
        .unwrap();

    assert!(session.holds(entry));
}

#[tokio::test]
async fn given_an_open_response_when_the_session_discards_it_then_nothing_is_stored_and_it_is_no_longer_tracked()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let _stream = session.open_response(&turn, entry).await.unwrap();

    session.discard_response(entry).await.unwrap();

    assert_eq!(session.open_objects(), 0);
    assert!(!session.holds(entry));
    assert!(world.access.conversation().turns().is_empty());
}

#[tokio::test]
async fn given_a_failed_model_call_when_the_instance_retries_then_the_new_response_has_a_new_stream_and_new_identities()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let (first, second) = (entry_id(), entry_id());
    let _ = session.open_response(&turn, first).await.unwrap();
    session.discard_response(first).await.unwrap();

    let _ = session.open_response(&turn, second).await.unwrap();
    session
        .complete_response(&turn, response_text(second, "retry"))
        .await
        .unwrap();

    assert!(!session.holds(first));
    assert!(session.holds(second));
}

#[tokio::test]
async fn given_a_tool_call_when_its_result_is_opened_and_completed_then_the_tool_result_is_stored_as_one_entry_that_holds_the_result_of_that_call()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, call) = world.open_turn_with_call(&mut session).await;
    let entry = entry_id();
    let mut stream = session.open_result(&turn, entry).await.unwrap();
    stream.send("part".to_owned()).await;

    session
        .complete_result(&turn, tool_results(entry, call, "lookup", "whole"))
        .await
        .unwrap();

    assert!(session.holds(entry));
    assert_eq!(session.open_objects(), 0);
    assert!(world.access.conversation().entry(&entry).is_ok());
}

#[tokio::test]
async fn given_a_tool_result_that_the_rules_refuse_when_it_is_completed_then_the_open_result_is_discarded_and_the_refusal_is_a_fault()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, _call) = world.open_turn_with_call(&mut session).await;
    let entry = entry_id();
    let _stream = session.open_result(&turn, entry).await.unwrap();

    let error = session
        .complete_result(&turn, tool_results(entry, block_id(), "lookup", "no call"))
        .await
        .unwrap_err();

    assert!(matches!(error, SessionError::Rules(_)));
    assert_eq!(session.open_objects(), 0);
}

#[tokio::test]
async fn given_open_objects_when_the_session_discards_all_then_every_object_is_discarded_and_nothing_is_added()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, _call) = world.open_turn_with_call(&mut session).await;
    let (response, result) = (entry_id(), entry_id());
    let _a = session.open_response(&turn, response).await.unwrap();
    let _b = session.open_result(&turn, result).await.unwrap();
    let before = world.access.conversation();

    let errors = session.discard_all().await;

    assert!(errors.is_empty());
    assert_eq!(session.open_objects(), 0);
    assert!(!world.access.is_open(response) && !world.access.is_open(result));
    assert_eq!(world.access.conversation(), before);
}

#[tokio::test]
async fn given_a_thread_access_that_fails_when_the_session_stores_an_addition_then_it_is_a_fault_and_the_call_is_not_retried()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    flaky.fail_adds.store(true, Ordering::SeqCst);

    let error = session
        .add_entry(
            &world.ai(turn_id()),
            TurnEntry::from(response_text(entry_id(), "x")),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, SessionError::Port(PortError::Fault(_))));
    assert_eq!(flaky.failed_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn given_a_refusal_by_the_thread_access_when_the_session_stores_an_addition_then_the_session_reports_it_as_an_error_of_the_instance_to_be_treated_as_a_fault()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    flaky.refuse_adds.store(true, Ordering::SeqCst);

    let error = session
        .add_entry(
            &world.ai(turn_id()),
            TurnEntry::from(response_text(entry_id(), "x")),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, SessionError::Port(PortError::Refused(_))));
}
