use br_graph_contract::Event;
use br_llm_messages::ClosureReason;

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_a_call_at_the_step_limit_when_the_agent_runs_then_the_turn_is_budget_exhausted_and_the_tool_is_not_run()
 {
    let (search, probe) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(2);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("search"), Reply::call("search")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    let turn = ai_turns(&conversation)[0];
    let reason = closure(turn).unwrap();
    assert!(matches!(reason, ClosureReason::BudgetExhausted { .. }));
    assert!(reason.detail().unwrap().contains("step limit"));
    assert_eq!(responses(turn).len(), 2);
    assert_eq!(results(turn).len(), 1);
    assert_eq!(model.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_at_the_step_limit_when_the_agent_runs_then_the_turn_is_completed_and_the_flag_is_not_set()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(2);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("search"), Reply::text("done")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert!(!update.step_limit);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(matches!(
        closure(ai_turns(&conversation)[0]),
        Some(ClosureReason::Completed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_provider_continuation_at_the_step_limit_when_the_agent_runs_then_the_limit_stops_it()
 {
    let rig = Rig::new().with_step_limit(1);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::Continuation]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(matches!(
        closure(ai_turns(&conversation)[0]),
        Some(ClosureReason::BudgetExhausted { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_provider_continuation_before_the_limit_when_the_agent_runs_then_the_next_step_follows_with_no_tool_round()
 {
    let rig = Rig::new().with_step_limit(5);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::Continuation, Reply::text("done")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert!(!update.step_limit);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(
        responses(ai_turns(&conversation)[0]),
        vec!["continuing", "done"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_step_limit_of_twelve_when_tool_rounds_end_then_the_warning_first_shows_with_ten_steps_left_and_is_never_stored()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(12);
    rig.start_with("go").await;
    let mut replies: Vec<Reply> = (0..11).map(|_| Reply::call("search")).collect();
    replies.push(Reply::text("done"));
    let model = Scripted::new(replies);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let requests = model.requests();
    let warned: Vec<bool> = requests
        .iter()
        .map(|request| history_text(request).contains("steps left"))
        .collect();
    assert_eq!(&warned[..2], &[false, false]);
    assert!(warned[2..11].iter().all(|shown| *shown), "{warned:?}");
    assert!(history_text(&requests[10]).contains("2 steps left"));
    assert!(history_text(&requests[11]).contains("one step left"));
    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = format!("{:?}", conversation);
    assert!(!stored.contains("steps left"));
    assert!(!stored.contains("one step left"));
}

#[tokio::test(start_paused = true)]
async fn given_a_step_limit_above_ten_when_far_from_the_limit_then_no_warning_is_sent() {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(100);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("search"), Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert!(!history_text(&model.requests()[1]).contains("steps left"));
}

#[tokio::test(start_paused = true)]
async fn given_a_node_run_twice_when_the_second_run_starts_then_the_step_counter_starts_again() {
    let rig = Rig::new().with_step_limit(1);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("first"), Reply::text("second")]);
    let node = rig.react(&[&model]);

    let (_, one) = Driver::run(&node).await;
    let (_, two) = Driver::run(&node).await;

    assert!(matches!(one, Event::Result(_)));
    assert!(matches!(two, Event::Result(_)));
}
