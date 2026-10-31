use br_graph_contract::Event;

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_a_model_that_answers_when_the_agent_runs_then_the_instance_ends_with_the_answer() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("hello")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "hello");
    assert_eq!(update.steps, Some(1));
    assert!(!update.step_limit);
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(turns[0]), vec!["hello".to_owned()]);
    assert!(closure(turns[0]).is_some());
}

#[tokio::test(start_paused = true)]
async fn given_a_pair_when_the_critic_validates_then_the_judgement_is_validated() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = rig.pair(one(&generator), one(&critic), 3);

    let (driver, event) = Driver::run(&node).await;
    let _ = driver;

    match event {
        Event::Result(update) => {
            assert_eq!(update.text, "draft");
            assert_eq!(update.iterations, Some(1));
        }
        other => panic!("{other:?}"),
    }
}
