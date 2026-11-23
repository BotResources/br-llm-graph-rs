use br_agentic_contract::memory::Call;

use crate::common::*;
use crate::kit::*;

#[tokio::test(start_paused = true)]
async fn given_a_pair_when_it_starts_then_the_tools_port_gets_one_run_context_of_the_generator_seat()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let contexts: Vec<_> = rig
        .record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::ToolsPort { context } => Some(context),
            _ => None,
        })
        .collect();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].thread, rig.thread);
    assert_eq!(contexts[0].agent, rig.generator);
    assert_eq!(contexts[0].author, author("Generator"));
    assert_eq!(contexts[0].conversation, rig.conversation_id);
}
