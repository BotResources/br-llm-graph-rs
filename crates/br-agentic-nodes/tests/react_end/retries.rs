use std::time::Duration;

use br_agentic_contract::FileSystemMode;
use br_graph_contract::Event;

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_returns_an_error_result_when_the_agent_runs_then_the_tool_is_not_retried_and_the_run_goes_on()
 {
    let (flaky, probe) = tool("flaky", Behavior::ErrorText("no luck".to_owned()));
    let rig = Rig::new().with_tools(vec![flaky]);
    let model = Scripted::new(vec![Reply::call("flaky"), Reply::text("end")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "end");
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(model.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_exceeds_its_timeout_when_the_agent_runs_then_the_tool_is_not_retried_and_the_run_goes_on()
 {
    let (stuck, probe) = tool("stuck", Behavior::Hang);
    let rig = Rig::new()
        .with_tools(vec![stuck])
        .with_timeouts(Duration::from_secs(5), Duration::from_millis(100));
    let model = Scripted::new(vec![Reply::call("stuck"), Reply::text("end")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "end");
    assert_eq!(*probe.starts.lock().unwrap(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_single_client_that_exceeds_its_timeout_when_the_agent_runs_then_the_call_is_not_retried_and_the_instance_fails()
 {
    let rig = Rig::new().with_timeouts(Duration::from_millis(100), Duration::from_secs(5));
    let model = Scripted::new(vec![Reply::Hang, Reply::text("never")]);
    let node = rig.react(&[&model]);

    let event = Driver::run(&node).await.1;

    assert!(matches!(event, Event::Failed(_)), "{event:?}");
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_first_client_that_exceeds_its_timeout_when_a_second_client_exists_then_only_the_second_client_is_tried_next()
 {
    let rig = Rig::new().with_timeouts(Duration::from_millis(100), Duration::from_secs(5));
    let slow = Scripted::new(vec![Reply::Hang]);
    let next = Scripted::new(vec![Reply::text("from the next")]);
    let node = rig.react(&[&slow, &next]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "from the next");
    assert_eq!(slow.calls(), 1);
    assert_eq!(next.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_and_an_outcome_that_work_when_the_agent_ends_then_each_port_is_called_once()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let seen = names(&rig.record);
    assert_eq!(
        seen.iter()
            .filter(|name| **name == "workspace_load")
            .count(),
        1
    );
    assert_eq!(
        seen.iter()
            .filter(|name| **name == "workspace_save")
            .count(),
        1
    );
    assert_eq!(
        seen.iter().filter(|name| **name == "thread_port").count(),
        1
    );
    assert_eq!(seen.iter().filter(|name| **name == "agent_port").count(), 1);
    assert_eq!(seen.iter().filter(|name| **name == "tools_port").count(), 1);
    assert_eq!(rig.prompts.asked("agent"), 1);
    assert_eq!(rig.outcome.calls(), 1);
}
