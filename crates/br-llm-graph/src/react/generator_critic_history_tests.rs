//! What the critic reads by default: `History` (each generation reduced to
//! its final step, no tool traffic, no thinking), `Latest` (up to the
//! trigger) and `Brief` (a conversation the caller fills), each ending with
//! the wrapped answer; the stable beginning each keeps across requests.

use std::sync::Arc;

use br_llm_messages::{
    Conversation, ImageMime, InputId, Text, UserBlock, UserInput, UserSource, WireMessage,
    WireUserBlock,
};
use serde_json::json;

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::generator_critic_text_tests::{opening, wrapped};
use crate::react::helpers::wire;
use crate::react::seats::{CriticHistory, CriticSeat, GeneratorSeat};
use crate::react::test_support::*;
use crate::state::{Kind, Value};

const REQUEST: &str = "Please evaluate the generator's response above.";

/// Three generations, the first two rejected, the first with a tool round
/// and thinking, judged by a critic reading `history`.
async fn three_generations(
    history: CriticHistory,
    extra: Vec<(crate::value::Key, Value)>,
) -> Vec<br_llm_clients_core::ClientRequest> {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        thought_step("plan", "first"),
        text_step("second"),
        text_step("third"),
    ]);
    let critic_client = mock(vec![
        verdict(false, "More."),
        verdict(false, "Still more."),
        verdict(true, ""),
    ]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool)],
            ..generator(&generator_client)
        },
        CriticSeat {
            history,
            ..critic(&critic_client)
        },
        Count::Fixed(3),
    )
    .graph()
    .unwrap();
    ended(start(&graph, extra).await, "validated");
    requests(&critic_client)
}

fn critique(text: &str) -> WireMessage {
    user_message(&format!(
        "<message author=\"reviewer\" role=\"agent\">{text}</message>"
    ))
}

#[tokio::test]
async fn given_the_history_default_when_the_critic_judges_then_it_reads_each_generation_reduced_to_its_final_step()
 {
    let requests = three_generations(CriticHistory::History, Vec::new()).await;

    assert_eq!(
        requests[0].messages(),
        [opening(), wrapped("first", REQUEST)]
    );
    assert_eq!(
        requests[2].messages(),
        [
            opening(),
            assistant_message(vec![text_block("first")]),
            critique("More."),
            assistant_message(vec![text_block("second")]),
            critique("Still more."),
            wrapped("third", REQUEST),
        ],
        "no tool call, no tool result, no thinking of the generator"
    );
    // A stable beginning: everything before the previous answer it judged.
    for pair in requests.windows(2) {
        let earlier = &pair[0].messages()[..pair[0].messages().len() - 1];
        assert!(pair[1].messages().starts_with(earlier));
    }
}

#[tokio::test]
async fn given_the_latest_default_when_the_critic_judges_then_it_reads_up_to_the_trigger_then_the_answer()
 {
    let requests = three_generations(CriticHistory::Latest, Vec::new()).await;

    for (request, answer) in requests.iter().zip(["first", "second", "third"]) {
        assert_eq!(request.messages(), [opening(), wrapped(answer, REQUEST)]);
    }
}

#[tokio::test]
async fn given_the_latest_default_on_an_opening_with_earlier_entries_when_the_critic_judges_then_they_are_kept_up_to_the_trigger()
 {
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("answer")])),
        CriticSeat {
            history: CriticHistory::Latest,
            ..critic(&critic_client)
        },
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let mut chat = question("earlier");
    chat.push_turn(br_llm_messages::Turn::new(
        br_llm_messages::TurnId::new("g0").unwrap(),
        Some(writer()),
        thought_step("old plan", "earlier answer"),
    ))
    .unwrap();
    for input in question("explain").entries() {
        let br_llm_messages::Entry::UserInput(input) = input else {
            continue;
        };
        chat.push_input(input.clone()).unwrap();
    }

    ended(call(&graph, chat).await, "validated");

    assert_eq!(
        requests(&critic_client)[0].messages(),
        [
            user_message("<message role=\"user\">earlier</message>"),
            assistant_message(vec![text_block("earlier answer")]),
            opening(),
            wrapped("answer", REQUEST),
        ]
    );
}

#[tokio::test]
async fn given_the_brief_default_when_the_critic_judges_then_it_reads_the_brief_rendered_for_it_then_the_answer()
 {
    let mut brief = Conversation::new();
    brief
        .push_input(
            UserInput::new(
                InputId::new("b1").unwrap(),
                UserSource::Human,
                None,
                vec![
                    UserBlock::text(Text::new("The chart the answer must explain.").unwrap()),
                    UserBlock::image(
                        ImageMime::Png,
                        br_llm_messages::Base64Data::new("iVBORw0KGgo=").unwrap(),
                    ),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let generator_client = mock(vec![text_step("first"), text_step("second")]);
    let critic_client = mock(vec![verdict(false, "More."), verdict(true, "")]);
    let graph = GeneratorCritic::new(
        generator(&generator_client),
        CriticSeat {
            history: CriticHistory::Brief(skey("brief")),
            ..critic(&critic_client)
        },
        Count::Fixed(2),
    )
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("brief")),
        Some(&Kind::Conversation)
    );

    ended(
        start(
            &graph,
            vec![(key("brief"), Value::conversation(brief.clone()))],
        )
        .await,
        "validated",
    );

    let rendered = wire(&brief, &reviewer()).unwrap();
    for (request, answer) in requests(&critic_client).iter().zip(["first", "second"]) {
        let mut expected = rendered.clone();
        expected.push(wrapped(answer, REQUEST));
        assert_eq!(request.messages(), expected.as_slice());
    }
    assert!(
        requests(&generator_client)
            .iter()
            .all(|request| !request.messages().iter().any(|message| matches!(
                message,
                WireMessage::User { content } if content.iter().any(|block| matches!(block, WireUserBlock::Image { .. }))
            ))),
        "only the critic reads the brief"
    );
}

#[tokio::test]
async fn given_the_generators_default_history_when_it_answers_again_then_its_request_extends_the_previous_one()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("first"),
        text_step("second"),
    ]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool)],
            ..generator(&generator_client)
        },
        critic(&mock(vec![verdict(false, "More."), verdict(true, "")])),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    ended(call(&graph, question("explain")).await, "validated");

    let requests = requests(&generator_client);
    for pair in requests.windows(2) {
        assert!(pair[1].messages().starts_with(pair[0].messages()));
    }
}
