use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::mails::{MAIL_MARKERS, deliver_mails};
use crate::pair::Pair;
use crate::prelude::*;
use crate::seen::{Seen, seen_by};

fn flat(request: &br_llm_clients_core::ClientRequest) -> Vec<Seen> {
    seen_by(request)
        .into_iter()
        .flat_map(|turn| turn.entries)
        .collect()
}

#[tokio::test(start_paused = true)]
async fn given_mails_and_notices_in_an_input_turn_when_the_critic_is_called_then_it_sees_none_and_keeps_the_human_message()
 {
    let rig = Rig::new().with_inbox();
    deliver_mails(&rig.inbox, rig.generator);
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let generator_view = history_text(&generator.requests()[0]);
    for marker in MAIL_MARKERS {
        assert!(generator_view.contains(marker), "{marker}");
    }
    let request = &critic.requests()[0];
    let seen = flat(request);
    assert!(!seen.contains(&Seen::Mail));
    assert!(!seen.contains(&Seen::Notice));
    let view = history_text(request);
    for marker in MAIL_MARKERS {
        assert!(!view.contains(marker), "{marker}");
    }
    assert!(
        seen.iter()
            .any(|entry| matches!(entry, Seen::Human { text, .. } if text == "start"))
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_input_turn_made_only_of_mails_when_the_critic_is_called_then_the_turn_is_dropped()
{
    let conversation_id = br_llm_messages::ConversationId::from_uuid(uuid()).unwrap();
    let rig = Rig::with_conversation(
        conversation_id,
        br_llm_messages::Conversation::new(conversation_id),
    )
    .with_inbox();
    deliver_mails(&rig.inbox, rig.generator);
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let generator_view = history_text(&generator.requests()[0]);
    assert!(generator_view.contains("SUBJECT-TO"));
    let request = &critic.requests()[0];
    let turns = seen_by(request);
    assert!(turns.iter().all(|turn| !turn.entries.is_empty()));
    assert!(matches!(
        turns.first().and_then(|turn| turn.entries.first()),
        Some(Seen::Output { text, .. }) if text == "draft"
    ));
    for marker in MAIL_MARKERS {
        assert!(!history_text(request).contains(marker), "{marker}");
    }
}

#[tokio::test(start_paused = true)]
async fn given_mails_and_notices_delivered_into_an_ai_turn_of_the_generator_when_the_critic_is_called_then_it_sees_none()
 {
    let (slow, _probe) = tool(
        "slow",
        Behavior::Slow(Duration::from_millis(10), "slept".to_owned()),
    );
    let rig = Rig::new().with_inbox().with_tools(vec![slow]);
    let generator = Scripted::new(vec![
        Reply::call("slow"),
        Reply::call("slow"),
        Reply::text("final answer"),
    ]);
    let inbox = rig.inbox.clone();
    let agent = rig.generator;
    let delivered = AtomicBool::new(false);
    generator.on_call(move |_| {
        if !delivered.swap(true, Ordering::SeqCst) {
            deliver_mails(&inbox, agent);
            inbox.deliver_entry(agent, inbox_entry("mid-pass human"));
        }
    });
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let last = generator.requests().pop().unwrap();
    let generator_view = history_text(&last);
    for marker in MAIL_MARKERS {
        assert!(generator_view.contains(marker), "{marker}");
    }
    assert!(generator_view.contains("mid-pass human"));
    let request = &critic.requests()[0];
    let seen = flat(request);
    assert!(!seen.contains(&Seen::Mail));
    assert!(!seen.contains(&Seen::Notice));
    for marker in MAIL_MARKERS {
        assert!(!history_text(request).contains(marker), "{marker}");
    }
    assert!(
        seen.iter()
            .any(|entry| matches!(entry, Seen::Human { text, .. } if text == "mid-pass human"))
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_generator_that_thinks_and_calls_tools_when_the_critic_is_called_then_it_sees_the_visible_output_only()
 {
    let (probe, _) = echo("probe-tool");
    let rig = Rig::new().with_tools(vec![probe]);
    let generator = Scripted::new(vec![
        Reply::thinking(Reply::call("probe-tool")),
        Reply::thinking(Reply::text("final answer")),
    ]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let generator_view = history_text(&generator.requests()[1]);
    assert!(generator_view.contains("probe-tool"));
    assert!(generator_view.contains("probe-tool done"));
    let request = &critic.requests()[0];
    let view = history_text(request);
    assert!(!view.contains("probe-tool"));
    assert!(!view.contains("\"hm\""));
    assert!(!view.contains("Thinking"));
    let outputs: Vec<_> = flat(request)
        .into_iter()
        .filter_map(|entry| match entry {
            Seen::Output {
                agent,
                author,
                text,
            } => Some((agent, author, text)),
            _ => None,
        })
        .collect();
    assert_eq!(
        outputs,
        vec![(
            rig.generator,
            "Generator".to_owned(),
            "final answer".to_owned()
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_earlier_message_of_the_critic_when_the_critic_is_called_again_then_it_sees_it_as_its_own_earlier_answer()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let requests = critic.requests();
    let first = seen_by(&requests[0]);
    assert!(first.iter().all(|turn| turn.ai_agent.is_none()));
    let second = seen_by(&requests[1]);
    let own: Vec<_> = second
        .iter()
        .filter(|turn| turn.ai_agent == Some(rig.critic))
        .collect();
    assert_eq!(own.len(), 1);
    assert_eq!(
        own[0].entries,
        vec![
            Seen::Answer {
                text: "improve".to_owned()
            },
            Seen::Other
        ]
    );
    let as_output = flat(&requests[1])
        .into_iter()
        .any(|entry| matches!(entry, Seen::Output { agent, .. } if agent == rig.critic));
    assert!(!as_output);
}
