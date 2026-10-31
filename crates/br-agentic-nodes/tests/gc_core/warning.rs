use br_agentic_nodes::warning_text;
use br_llm_clients_core::ClientRequest;

use crate::common::*;
use crate::kit::*;
use crate::view::*;

fn warned(request: &ClientRequest) -> Vec<u32> {
    let history = history_text(request);
    (1..=10)
        .filter(|left| history.contains(&warning_text(*left).unwrap()))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn given_two_passes_when_steps_run_low_then_only_the_steps_after_a_tool_run_carry_the_warning_and_it_is_never_stored()
 {
    let (tool, _) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(5);
    let generator = Scripted::new(vec![
        Reply::call("lookup"),
        Reply::text("d1"),
        Reply::call("lookup"),
        Reply::text("d2"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let seen: Vec<Vec<u32>> = generator.requests().iter().map(warned).collect();
    assert_eq!(seen, vec![vec![], vec![4], vec![], vec![2]]);
    assert!(
        critic
            .requests()
            .iter()
            .all(|request| warned(request).is_empty())
    );
    assert!(
        timeline(&rig)
            .iter()
            .all(|line| !line.contains("steps left"))
    );
}

#[tokio::test(start_paused = true)]
async fn given_one_step_left_after_a_tool_run_when_the_next_step_is_called_then_the_warning_asks_for_no_tool_and_the_tools_are_still_offered()
 {
    let (tool, _) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(2);
    let generator = Scripted::new(vec![Reply::call("lookup"), Reply::text("d1")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let last = generator.requests().pop().unwrap();
    assert_eq!(warned(&last), vec![1]);
    assert_eq!(last.tools().len(), 1);
}
