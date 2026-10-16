//! Nothing a function computes, no prompt text, no client, no tool argument
//! and no secret is part of a description.

use std::sync::{Arc, LazyLock};

use br_llm_clients_core::{
    OutputMode, Parameter, ParameterKind, Parameters, StructuredShape, ToolSpec,
};
use br_llm_messages::{Text, ToolName, ToolResultBlock};
use serde_json::Value as Json;

use crate::graph::{Always, GraphBuilder, Input, SubGraph, Target};
use crate::react::generator_critic_support::{duo, nid};
use crate::react::given::{Model, Prompt, Reads};
use crate::react::llm_call::{LlmCall, LlmOutput, LlmRequest};
use crate::react::react_agent_support::{agent, question};
use crate::react::test_support::{key, mock, skey};
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::EndLabel;

/// A tool whose description and parameters hold a secret.
struct Vault;

impl Tool for Vault {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| {
            let parameters = Parameters::new(vec![
                Parameter::new(
                    "SECRET_PARAMETER",
                    "SECRET-PARAMETER-TEXT",
                    true,
                    ParameterKind::String,
                )
                .unwrap(),
            ])
            .unwrap();
            ToolSpec::new(
                ToolName::new("vault").unwrap(),
                "SECRET-TOOL-TEXT",
                parameters,
            )
            .unwrap()
        });
        &SPEC
    }

    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("SECRET-RESULT").unwrap(),
            )]))
        })
    }
}

const SENTINELS: [&str; 8] = [
    "SECRET-SYSTEM",
    "SECRET-REQUEST",
    "SECRET-TOOL-TEXT",
    "SECRET_PARAMETER",
    "SECRET-PARAMETER-TEXT",
    "SECRET-CONST",
    "SECRET-REVIEW",
    "SECRET-SHAPE",
];

#[test]
fn given_prompts_tools_clients_and_fixed_values_when_described_then_none_of_them_is_written() {
    let client = Arc::new(mock(Vec::new()));
    let mut researcher = agent(&client);
    researcher.system = Prompt::text("SECRET-SYSTEM");
    researcher.tools = vec![Arc::new(Vault)];
    let researcher = researcher.graph().unwrap();

    let mut critic_duo = duo(&mock(Vec::new()), &mock(Vec::new()));
    critic_duo.critic.review_request = Some(Prompt::text("SECRET-REVIEW"));
    critic_duo.generator.output_mode = OutputMode::Structured(
        StructuredShape::new(
            "SECRET-SHAPE",
            Some("SECRET-SHAPE text".to_owned()),
            Parameters::default(),
        )
        .unwrap(),
    );
    let critic_duo = critic_duo.graph().unwrap();

    let summarize = LlmCall {
        model: Model::from_fn(Reads::new(), move |_| {
            Ok(client.clone() as Arc<dyn br_llm_clients_core::Client>)
        }),
        system: Prompt::text("SECRET-SYSTEM"),
        request: LlmRequest::Text(Prompt::text("SECRET-REQUEST")),
        output_mode: OutputMode::Plain,
        output: LlmOutput::Set(skey("note")),
    };
    let schema = Schema::builder()
        .state_by_name(key("note"), Kind::Str)
        .state_by_name(key("question"), Kind::Conversation)
        .build();
    let done = || Always(Target::End(EndLabel::new("done").unwrap()));
    let graph = GraphBuilder::new(schema)
        .entry(nid("summarize"))
        .node(nid("summarize"), summarize)
        .node(
            nid("researcher"),
            SubGraph::call(researcher).input(
                key("opening"),
                Input::Const(Value::conversation(question("SECRET-CONST"))),
            ),
        )
        .node(
            nid("duo"),
            SubGraph::call(critic_duo).input(key("opening"), Input::From(key("question"))),
        )
        .edge(nid("summarize"), Always(Target::Node(nid("researcher"))))
        .edge(nid("researcher"), Always(Target::Node(nid("duo"))))
        .edge(nid("duo"), done())
        .build()
        .unwrap();
    let written = serde_json::to_string(&graph.describe()).unwrap();
    for sentinel in SENTINELS {
        assert!(
            !written.contains(sentinel),
            "{sentinel} is written: {written}"
        );
    }
    assert!(written.contains("\"tools\":[\"vault\"]"));
}
