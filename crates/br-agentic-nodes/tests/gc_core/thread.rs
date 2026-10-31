use crate::common::*;
use crate::kit::*;
use crate::view::*;

#[tokio::test(start_paused = true)]
async fn given_a_rejection_when_the_generator_passes_again_then_it_sees_the_message_as_output_of_the_critic()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let second = &generator.requests()[1];
    assert!(history_agent_outputs(second.history()).contains(&(rig.critic, "fix it".to_owned())));
    let first = &generator.requests()[0];
    assert!(history_agent_outputs(first.history()).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_two_rounds_when_the_critic_is_called_again_then_it_sees_its_own_message_as_its_own_turn_and_the_generator_as_another_agent()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let second = &critic.requests()[1];
    assert!(history_ai_turns(second.history()).contains(&(rig.critic, "fix it".to_owned())));
    let outputs = history_agent_outputs(second.history());
    assert!(outputs.contains(&(rig.generator, "better".to_owned())));
    assert!(outputs.iter().all(|(agent, _)| *agent != rig.critic));
}

#[tokio::test(start_paused = true)]
async fn given_a_rejection_when_the_pair_runs_then_the_generator_and_the_critic_write_into_the_one_thread()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:draft",
            "critic[Completed]:fix it",
            "generator[Completed]:better"
        ]
    );
}
