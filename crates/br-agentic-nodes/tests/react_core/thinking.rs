use br_llm_messages::ResponseEnding;

use crate::common::*;
use crate::kit::*;

async fn answered_with(parts: Vec<Part>) -> Recorded {
    let rig = Rig::new();
    let model = Model::new(vec![Act::shaped(parts, ResponseEnding::finished())]);
    let node = node(&rig, chain(vec![expecting(&model)]));
    Driver::result(&node).await
}

async fn ended_with(ending: ResponseEnding) -> (Recorded, u32) {
    let rig = Rig::new();
    let model = Model::new(vec![Act::ended("no", ending)]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, chain(vec![expecting(&model)]), &capture);
    let update = Driver::result(&node).await;
    (update, capture.only().steps())
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_first_response_has_no_thinking_then_the_next_client_answers_and_the_open_response_is_discarded()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::text("mute")]);
    let second = Model::new(vec![Act::text("fine")]);
    let node = node(&rig, chain(vec![expecting(&first), plain(&second)]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "fine");
    assert_eq!(
        discarded_responses(&rig.record),
        vec![first.requests()[0].reply_entry()]
    );
    assert_eq!(
        response_ids(&rig.conversation()),
        vec![second.requests()[0].reply_entry()]
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_alone_when_the_first_response_has_no_thinking_then_the_call_is_a_fault()
 {
    let rig = Rig::new();
    let only = Model::new(vec![Act::text("mute")]);
    let node = node(&rig, chain(vec![expecting(&only)]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("think"), "{failure}");
    assert_eq!(only.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_is_not_expected_to_think_when_the_response_has_no_thinking_then_the_call_works()
 {
    let rig = Rig::new();
    let only = Model::new(vec![Act::text("mute")]);
    let node = node(&rig, plains(&[&only]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "mute");
}

#[tokio::test(start_paused = true)]
async fn given_a_list_when_the_declaration_differs_per_client_then_each_client_follows_its_own_declaration()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::text("mute")]);
    let second = Model::new(vec![Act::text("never")]);
    let node = node(&rig, chain(vec![plain(&first), expecting(&second)]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "mute");
    assert_eq!(second.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_response_holds_a_thinking_block_then_the_call_works() {
    let update = answered_with(vec![Part::Thinking, Part::Text("ok".to_owned())]).await;

    assert_eq!(update.text, "ok");
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_response_holds_a_redacted_thinking_block_then_the_call_works()
 {
    let update = answered_with(vec![Part::Redacted, Part::Text("ok".to_owned())]).await;

    assert_eq!(update.text, "ok");
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_response_holds_a_thinking_block_with_empty_text_then_the_call_works()
 {
    let update = answered_with(vec![Part::EmptyThinking, Part::Text("ok".to_owned())]).await;

    assert_eq!(update.text, "ok");
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_a_later_step_has_no_thinking_then_the_step_passes() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let only = Model::new(vec![
        Act::call("alpha").thinking(),
        Act::call("alpha"),
        Act::text("done"),
    ]);
    let node = node(&rig, chain(vec![expecting(&only)]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert_eq!(update.steps, Some(3));
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_first_response_is_a_refusal_without_tool_call_then_it_is_a_final_response()
 {
    let (update, steps) = ended_with(ResponseEnding::refusal()).await;

    assert_eq!((update.text.as_str(), steps), ("no", 1));
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_first_response_is_a_filter_without_tool_call_then_it_is_a_final_response()
 {
    let (update, steps) = ended_with(ResponseEnding::filtering()).await;

    assert_eq!((update.text.as_str(), steps), ("no", 1));
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_the_first_response_is_an_exhaustion_without_tool_call_then_it_is_a_final_response()
 {
    let (update, steps) = ended_with(ResponseEnding::output_exhaustion()).await;

    assert_eq!((update.text.as_str(), steps), ("no", 1));
}

#[tokio::test(start_paused = true)]
async fn given_an_expecting_client_when_a_refusal_asks_for_a_tool_call_then_it_is_not_exempt_and_the_call_fails()
 {
    let rig = Rig::new();
    let parts = vec![Part::Call("alpha".to_owned(), "{}".to_owned())];
    let only = Model::new(vec![Act::shaped(parts, ResponseEnding::refusal())]);
    let node = node(&rig, chain(vec![expecting(&only)]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("think"), "{failure}");
}
