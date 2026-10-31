use std::time::Duration;

use br_agentic_nodes::SEND_MAIL;

use crate::common::*;
use crate::kit::*;
use crate::mail_send_tool::{run_sends, send_arguments, sent_mails};
use crate::spy::{Act, Spy};

#[tokio::test(start_paused = true)]
async fn given_a_mail_without_recipient_in_to_when_the_inbox_refuses_then_the_result_is_an_error_with_the_reason()
 {
    let rig = Rig::new();
    let arguments = send_arguments(&[], &[agent_id()], "s", "b", &[]);

    let results = run_sends(&rig, vec![Reply::call_with(SEND_MAIL, &arguments)]).await;

    assert!(results[0].is_error);
    assert!(
        results[0].text().contains("at least one recipient in to"),
        "{}",
        results[0].text()
    );
    assert!(rig.inbox.mail(sent_mails(&rig)[0].1.id).is_none());
}

#[tokio::test(start_paused = true)]
async fn given_a_recipient_listed_twice_when_the_inbox_refuses_then_the_result_is_an_error_with_the_reason()
 {
    let rig = Rig::new();
    let twice = agent_id();
    let arguments = send_arguments(&[twice], &[twice], "s", "b", &[]);

    let results = run_sends(&rig, vec![Reply::call_with(SEND_MAIL, &arguments)]).await;

    assert!(results[0].is_error);
    assert!(
        results[0].text().contains("listed twice"),
        "{}",
        results[0].text()
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_inbox_that_refuses_with_a_reason_when_a_mail_is_sent_then_the_result_carries_the_reason_and_the_run_goes_on()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    spy.plan_sends(vec![Act::Refuse("recipient is on leave".to_owned())]);
    let arguments = send_arguments(&[agent_id()], &[], "s", "b", &[]);
    let model = Scripted::new(vec![
        Reply::call_with(SEND_MAIL, &arguments),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert!(results[0].is_error);
    assert!(results[0].text().contains("recipient is on leave"));
}

#[tokio::test(start_paused = true)]
async fn given_an_inbox_that_faults_when_a_mail_is_sent_then_the_result_is_an_error_and_the_run_goes_on()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    spy.plan_sends(vec![Act::Fault]);
    let arguments = send_arguments(&[agent_id()], &[], "s", "b", &[]);
    let model = Scripted::new(vec![
        Reply::call_with(SEND_MAIL, &arguments),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert!(results[0].is_error);
    assert!(!results[0].text().trim().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_send_that_exceeds_the_tool_timeout_when_the_mail_is_sent_then_the_text_says_it_is_not_known_whether_the_mail_was_sent()
 {
    let rig = Rig::new().with_timeouts(Duration::from_secs(5), Duration::from_secs(1));
    let spy = Spy::new(&rig);
    spy.plan_sends(vec![Act::Hang]);
    let arguments = send_arguments(&[agent_id()], &[], "s", "b", &[]);
    let model = Scripted::new(vec![
        Reply::call_with(SEND_MAIL, &arguments),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert!(results[0].is_error);
    let text = results[0].text();
    assert!(text.contains("timeout"), "{text}");
    assert!(
        text.contains("not known whether the mail was sent"),
        "{text}"
    );
}

#[tokio::test(start_paused = true)]
async fn given_another_tool_that_exceeds_the_tool_timeout_when_it_runs_then_the_text_does_not_speak_of_a_mail()
 {
    let (slow, _) = tool("slow", Behavior::Hang);
    let rig = Rig::new()
        .with_tools(vec![slow])
        .with_timeouts(Duration::from_secs(5), Duration::from_secs(1));
    let model = Scripted::new(vec![Reply::call("slow"), Reply::text("done")]);
    let node = plain(&rig, &model, "Agent");

    Driver::result(&node).await;

    let results = outcomes(&rig.conversation());
    assert!(results[0].is_error);
    assert!(results[0].text().contains("timeout"));
    assert!(!results[0].text().contains("whether the mail was sent"));
}
