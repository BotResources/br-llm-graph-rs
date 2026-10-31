use br_graph_contract::Event;

use crate::common::*;
use crate::drive::*;
use crate::kit::{Parts, react_spec};
use crate::ports_kit::*;

#[tokio::test(start_paused = true)]
async fn given_two_inputs_given_before_the_run_when_the_agent_runs_then_only_the_first_input_is_used()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("done")]);
    let prompts = SnapshotPrompt::new();
    let outcome = SnapshotOutcome::new();
    let mut parts = Parts::of(&rig);
    parts.prompt = prompts.clone();
    let node = react_spec(&rig, parts, list(&[&model]), outcome.clone(), None);
    let mut driver = Driver::start(&node).await;

    driver.input.give(input_with("first")).unwrap();
    driver.input.give(input_with("second")).unwrap();
    let event = driver.event().await;

    assert!(matches!(event, Event::Result(_)), "{event:?}");
    assert_eq!(prompts.notes(), vec!["first".to_owned()]);
    assert_eq!(outcome.notes(), vec!["first".to_owned()]);
    assert_eq!(model.calls(), 1);
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_run_that_ends_with_a_result_when_the_event_is_sent_then_the_input_end_is_closed() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    let (driver, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Result(_)));
    assert!(input_is_closed(&driver));
}
