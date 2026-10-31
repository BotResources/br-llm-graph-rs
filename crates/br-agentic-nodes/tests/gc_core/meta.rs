use std::sync::Arc;

use br_agentic_contract::memory::Call;
use br_agentic_nodes::AuthorSource;
use br_graph_contract::Node;

use crate::access::*;
use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

fn labels(rig: &Rig) -> Vec<String> {
    let mut found: Vec<String> = rig
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::AgentPort { label, .. } => Some(label.as_str().to_owned()),
            _ => None,
        })
        .collect();
    found.sort();
    found
}

#[test]
fn given_a_pair_when_asked_for_its_identity_then_it_is_an_agentic_generator_critic() {
    let rig = Rig::new();
    let (generator, critic) = (Scripted::new(vec![]), Scripted::new(vec![]));
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    assert_eq!(node.kind(), "agentic");
    assert_eq!(node.subkind(), "generator_critic");
    assert_eq!(node.name(), "pair");
}

#[tokio::test(start_paused = true)]
async fn given_no_programmatic_critic_when_the_pair_runs_then_the_agent_port_is_asked_once_for_each_of_generator_and_critic()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("d1"),
        Reply::text("d2"),
        Reply::text("d3"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "m1"),
        Reply::answer(false, "m2"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 5, Setup::default());

    outcome_of(&node).await;

    assert_eq!(
        labels(&rig),
        vec!["critic".to_owned(), "generator".to_owned()]
    );
}

#[tokio::test(start_paused = true)]
async fn given_three_rounds_when_the_pair_runs_then_the_prompt_port_is_asked_once_per_agent_and_each_agent_uses_its_prompt()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("d1"),
        Reply::text("d2"),
        Reply::text("d3"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "m1"),
        Reply::answer(false, "m2"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 5, Setup::default());

    outcome_of(&node).await;

    assert_eq!(rig.prompts.asked("generator"), 1);
    assert_eq!(rig.prompts.asked("critic"), 1);
    assert_eq!(rig.prompts.asked("agent"), 0);
    assert_eq!(rig.prompts.asked("programmatic_critic"), 0);
    for request in generator.requests() {
        assert_eq!(
            request.system().unwrap().as_str(),
            "system prompt of the generator"
        );
    }
    for request in critic.requests() {
        assert_eq!(
            request.system().unwrap().as_str(),
            "system prompt of the critic"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn given_tools_when_the_critic_is_called_then_it_is_offered_none_and_the_generator_is_offered_them()
 {
    let (tool, _) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]);
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(generator.requests()[0].tools().len(), 1);
    assert!(critic.requests()[0].tools().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_critic_prompt_when_the_pair_starts_then_it_fails_before_the_first_step_of_the_generator()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let spec = rig.pair_spec(one(&generator), one(&critic), 3);
    let ports = ports_over(&rig, rig.access.clone(), Arc::new(BlankPromptFor("critic")));
    let node = pair_over(&rig, ports, spec);

    let reason = failed(&node).await;

    assert!(!reason.is_empty());
    assert_eq!(generator.calls(), 0);
    assert_eq!(critic.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_generator_prompt_when_the_pair_starts_then_it_fails_before_the_first_step_of_the_generator()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let spec = rig.pair_spec(one(&generator), one(&critic), 3);
    let ports = ports_over(
        &rig,
        rig.access.clone(),
        Arc::new(BlankPromptFor("generator")),
    );
    let node = pair_over(&rig, ports, spec);

    failed(&node).await;

    assert_eq!(generator.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_author_ports_when_two_rounds_run_then_each_agent_is_asked_once_at_start() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
    let authors = Arc::new(Authors::default());
    let mut spec = rig.pair_spec(one(&generator), one(&critic), 3);
    spec.generator_author = AuthorSource::Port(authors.clone());
    spec.critic_author = AuthorSource::Port(authors.clone());
    let node = pair_over(&rig, rig.ports(), spec);

    outcome_of(&node).await;

    assert_eq!(authors.asked("generator"), 1);
    assert_eq!(authors.asked("critic"), 1);
    let conversation = rig.conversation();
    let by_role: Vec<(&str, String)> = ai_turns(&conversation)
        .iter()
        .map(|turn| (role(&rig, turn.agent()), turn.author().as_str().to_owned()))
        .collect();
    assert_eq!(
        by_role,
        vec![
            ("generator", "Author of the generator".to_owned()),
            ("critic", "Author of the critic".to_owned()),
            ("generator", "Author of the generator".to_owned()),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_refused_critic_author_when_the_pair_starts_then_it_fails_before_the_first_step_of_the_generator()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let mut spec = rig.pair_spec(one(&generator), one(&critic), 3);
    spec.critic_author = AuthorSource::Port(Arc::new(Authors::blank_for("critic")));
    let node = pair_over(&rig, rig.ports(), spec);

    failed(&node).await;

    assert_eq!(generator.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_items_for_the_critic_and_the_generator_when_the_pair_runs_then_only_the_generator_reads_its_inbox()
 {
    let rig = Rig::new().with_inbox();
    let (_, for_generator) = human_item(&rig, "for the generator");
    for_generator();
    let to_critic = br_agentic_contract::InboxEntry::new(br_llm_messages::IncomingEntry::from(
        human_message("for the critic"),
    ))
    .unwrap();
    let critic_item = to_critic.id();
    rig.inbox.deliver_entry(rig.critic, to_critic);
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let readers = read_for(&rig);
    assert!(!readers.is_empty());
    assert!(readers.iter().all(|agent| *agent == rig.generator));
    let story = timeline(&rig);
    assert!(story.contains(&"in:for the generator".to_owned()));
    assert!(!story.contains(&"in:for the critic".to_owned()));
    assert!(!rig.inbox.is_read(rig.critic, critic_item));
}
