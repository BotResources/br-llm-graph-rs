use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::{Conversation, Step};
use serde_json::{Value as Json, json};

use crate::error::GraphError;
use crate::graph::{Count, Switch};
use crate::react::generator_critic::{CriticSeat, GeneratorCritic};
use crate::react::generator_critic_tests::*;
use crate::react::llm_node::Source;
use crate::react::model::{OutputMode, Request};
use crate::react::test_support::*;
use crate::react::verdict::{Verdict, VerdictOverride};
use crate::state::{Kind, Value};
use crate::value::Key;

/// The critic's request on a one-round run whose answer it validates with
/// `reply`, and the generator-critic's state at its end.
async fn judged_with(
    critic: CriticSeat,
    reply: Step,
    config: BTreeMap<Key, Value>,
) -> (Request, crate::state::State) {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![reply]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat {
            model: critic_model.clone(),
            ..critic
        },
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let state = ended(
        call_with(&graph, question("explain"), Conversation::new(), config).await,
        "validated",
    );
    (critic_model.requests().remove(0), state)
}

fn seat() -> CriticSeat {
    critic(Arc::new(LoggingModel::new(Vec::new())))
}

fn valid() -> Step {
    verdict(true, "")
}

fn schema_of(request: &Request) -> &Json {
    match &request.output {
        OutputMode::Structured { schema } => schema,
        OutputMode::Text => panic!("expected a structured request"),
    }
}

fn property_names(schema: &Json) -> Vec<&str> {
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn default_schema() -> Json {
    json!({
        "type": "object",
        "properties": {
            "thinking": {
                "type": "string",
                "description": "Explanation of the thought process and validation steps"
            },
            "is_valid": {
                "type": "boolean",
                "description": "Boolean indicating if the generated message is valid"
            },
            "message": {
                "type": "string",
                "description": "Feedback message for the generator, empty if validation passed"
            }
        },
        "required": ["thinking", "is_valid", "message"],
        "additionalProperties": false
    })
}

#[tokio::test]
async fn given_the_default_critic_when_it_judges_then_thinking_is_valid_and_message_in_order_with_their_descriptions()
 {
    let (request, _) = judged_with(seat(), valid(), BTreeMap::new()).await;
    assert_eq!(schema_of(&request), &default_schema());
    assert_eq!(
        property_names(schema_of(&request)),
        ["thinking", "is_valid", "message"]
    );
    assert_eq!(request.thinking, None);
    assert!(request.tools.is_empty());
}

#[tokio::test]
async fn given_the_critic_thinking_switch_on_or_off_when_it_judges_then_the_schema_is_unchanged() {
    for thinking in [Switch::Fixed(true), Switch::Fixed(false)] {
        let (request, _) = judged_with(
            CriticSeat {
                thinking: Some(thinking),
                ..seat()
            },
            valid(),
            BTreeMap::new(),
        )
        .await;
        assert_eq!(schema_of(&request), &default_schema());
    }
    for deep in [true, false] {
        let (request, _) = judged_with(
            CriticSeat {
                thinking: Some(Switch::Config(key("critic_thinks"))),
                ..seat()
            },
            valid(),
            BTreeMap::from([(key("critic_thinks"), Value::bool(deep))]),
        )
        .await;
        assert_eq!(request.thinking, Some(deep));
        assert_eq!(schema_of(&request), &default_schema());
    }
}

#[tokio::test]
async fn given_no_reasoning_field_when_the_critic_judges_then_the_schema_has_no_thinking() {
    let (request, _) = judged_with(
        CriticSeat {
            reasoning_field: false,
            thinking_description: Some(Source::Const("never sent".to_owned())),
            ..seat()
        },
        structured_step(json!({ "is_valid": true, "message": "" })),
        BTreeMap::new(),
    )
    .await;
    let mut expected = default_schema();
    expected["properties"]
        .as_object_mut()
        .unwrap()
        .remove("thinking");
    expected["required"] = json!(["is_valid", "message"]);
    assert_eq!(schema_of(&request), &expected);
}

#[tokio::test]
async fn given_custom_descriptions_when_the_critic_judges_then_each_is_resolved_into_the_schema() {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![valid()]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat {
            thinking_description: Some(Source::Const("Your reasoning".to_owned())),
            is_valid_description: Some(Source::State(key("pass_rule"))),
            message_description: Some(Source::Config(key("feedback_rule"))),
            ..critic(critic_model.clone())
        },
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("pass_rule")),
        Some(&Kind::Str)
    );
    let state = graph
        .start_state([
            (
                key("conversation"),
                Value::conversation(question("explain")),
            ),
            (
                key("generator_history"),
                Value::conversation(Conversation::new()),
            ),
            (key("pass_rule"), Value::str("True when complete")),
        ])
        .unwrap();
    let config = crate::state::Config::new(
        graph.schema(),
        BTreeMap::from([(key("feedback_rule"), Value::str("What to change"))]),
    )
    .unwrap();
    let (_sender, mut inbox) = crate::run::channel();
    ended(
        crate::run::run(&graph, &config, state, None, &ctx(), &mut inbox).await,
        "validated",
    );
    let request = critic_model.requests().remove(0);
    let schema = schema_of(&request);
    assert_eq!(property_names(schema), ["thinking", "is_valid", "message"]);
    assert_eq!(
        schema["properties"]["thinking"]["description"],
        "Your reasoning"
    );
    assert_eq!(
        schema["properties"]["is_valid"]["description"],
        "True when complete"
    );
    assert_eq!(
        schema["properties"]["message"]["description"],
        "What to change"
    );
}

fn graded_schema() -> Json {
    json!({
        "type": "object",
        "properties": {
            "grade": { "type": "string", "enum": ["pass", "fail"] },
            "notes": { "type": "string" }
        },
        "required": ["grade", "notes"]
    })
}

fn graded_reader() -> crate::react::verdict::VerdictReader {
    Arc::new(|value| {
        let grade = value["grade"]
            .as_str()
            .ok_or_else(|| GraphError::Structured {
                message: "no grade".to_owned(),
            })?;
        Ok(Verdict {
            is_valid: grade == "pass",
            message: value["notes"].as_str().unwrap_or_default().to_owned(),
        })
    })
}

#[tokio::test]
async fn given_an_override_with_a_reader_when_the_critic_rejects_then_its_schema_is_sent_and_the_reader_gives_the_verdict()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![
        text_step("draft"),
        text_step("better"),
    ]));
    let critic_model = Arc::new(LoggingModel::new(vec![
        structured_step(json!({ "grade": "fail", "notes": "too short" })),
        structured_step(json!({ "grade": "pass", "notes": "" })),
    ]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat {
            reasoning_field: false,
            verdict: Some(VerdictOverride {
                schema: graded_schema(),
                reader: Some(graded_reader()),
            }),
            ..critic(critic_model.clone())
        },
        Count::Fixed(2),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(
        shape_of(&state, "conversation"),
        [
            "user",
            "writer:draft",
            "reviewer:too short",
            "writer:better"
        ]
    );
    for request in critic_model.requests() {
        assert_eq!(schema_of(&request), &graded_schema());
    }
}

#[tokio::test]
async fn given_a_reader_that_rejects_with_a_blank_message_when_the_critic_judges_then_structured() {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![structured_step(
        json!({ "grade": "fail", "notes": " \n" }),
    )]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat {
            verdict: Some(VerdictOverride {
                schema: graded_schema(),
                reader: Some(graded_reader()),
            }),
            ..critic(critic_model)
        },
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let failure = call(&graph, question("explain")).await.err().unwrap();
    assert!(
        failure.error.to_string().contains("blank message"),
        "{}",
        failure.error
    );
}

#[tokio::test]
async fn given_an_override_without_a_reader_when_the_critic_judges_then_the_schema_is_sent_as_given()
 {
    let schema = json!({
        "type": "object",
        "properties": {
            "score": { "type": "integer" },
            "is_valid": { "type": "boolean" },
            "message": { "type": "string" }
        },
        "required": ["score", "is_valid", "message"]
    });
    let (request, _) = judged_with(
        CriticSeat {
            verdict: Some(VerdictOverride {
                schema: schema.clone(),
                reader: None,
            }),
            message_description: Some(Source::State(key("ignored"))),
            ..seat()
        },
        structured_step(json!({ "score": 9, "is_valid": true, "message": "" })),
        BTreeMap::new(),
    )
    .await;
    assert_eq!(schema_of(&request), &schema);
}

#[test]
fn given_an_override_schema_that_cannot_be_read_without_a_reader_when_built_then_verdict_schema_mismatch()
 {
    let base = || {
        json!({
            "type": "object",
            "properties": {
                "is_valid": { "type": "boolean" },
                "message": { "type": "string" }
            },
            "required": ["is_valid", "message"]
        })
    };
    let mut not_object = base();
    not_object["type"] = json!("array");
    let mut no_message = base();
    no_message["properties"]
        .as_object_mut()
        .unwrap()
        .remove("message");
    let mut string_flag = base();
    string_flag["properties"]["is_valid"] = json!({ "type": "string" });
    let mut optional_message = base();
    optional_message["required"] = json!(["is_valid"]);
    for schema in [
        json!("verdict"),
        not_object,
        no_message,
        string_flag,
        optional_message,
    ] {
        let built = GeneratorCritic::new(
            generator(Arc::new(LoggingModel::new(Vec::new()))),
            CriticSeat {
                verdict: Some(VerdictOverride {
                    schema: schema.clone(),
                    reader: None,
                }),
                ..seat()
            },
            Count::Fixed(1),
        )
        .graph();
        assert!(
            matches!(built, Err(GraphError::VerdictSchemaMismatch { .. })),
            "{schema}"
        );
    }
    let accepted = GeneratorCritic::new(
        generator(Arc::new(LoggingModel::new(Vec::new()))),
        CriticSeat {
            verdict: Some(VerdictOverride {
                schema: json!("anything"),
                reader: Some(graded_reader()),
            }),
            ..seat()
        },
        Count::Fixed(1),
    )
    .graph();
    assert!(accepted.is_ok());
}
