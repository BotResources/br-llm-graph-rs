use br_agentic_contract::{AgentLabel, ThreadAccess};
use br_agentic_harness::HarnessHistory;

use crate::common::*;

#[tokio::test]
async fn given_a_runner_prompt_when_the_node_reads_it_through_the_harness_then_it_gets_the_prompt_of_the_runner_unchanged()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    let prompt = harness
        .prompt()
        .prompt(&Doc { n: 0 }, &AgentLabel::new("main")?)
        .await?;

    assert_eq!(prompt, "the prompt of the runner");
    Ok(())
}

#[tokio::test]
async fn given_an_entry_written_through_the_harness_history_when_the_thread_is_read_then_the_runner_history_holds_it_and_the_node_sees_no_change()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = entry_id()?;

    harness
        .history()
        .add_input_turn(&world.thread, mail_turn(id)?)
        .await?;

    let through_harness = harness.history().read(&world.thread).await?;
    let direct = world.history.read(&world.thread).await?;
    assert_eq!(through_harness, direct);
    assert!(direct.entry(&id).is_ok());
    Ok(())
}

#[tokio::test]
async fn given_a_write_the_runner_history_refuses_when_it_goes_through_the_harness_then_the_node_gets_the_same_refusal()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let other = thread_ref()?;
    let direct = world
        .history
        .add_input_turn(&other, mail_turn(entry_id()?)?)
        .await;

    let through = harness
        .history()
        .add_input_turn(&other, mail_turn(entry_id()?)?)
        .await;

    assert_eq!(direct.is_err(), through.is_err());
    assert!(through.is_err());
    Ok(())
}

#[tokio::test]
async fn given_a_response_written_through_the_harness_when_the_stored_signal_is_asked_then_the_runner_history_answers_stored()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = entry_id()?;

    harness
        .history()
        .add_entry(&world.thread, world.turn()?, response_entry(id)?)
        .await?;

    let answer = world.history.stored(&world.thread, id).await?;
    assert_eq!(answer, br_agentic_harness_contract::StoredAnswer::Stored);
    Ok(())
}
