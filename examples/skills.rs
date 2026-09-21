#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use br_llm_graph::{
    Config, EndLabel, GraphBuilder, Kind, LlmNode, Model, ModelError, ModelFuture, NodeId, Outcome,
    OutputMode, ReactLoop, Request, Schema, Source, State, StreamSink, Target, Tool, ToolFuture,
    ToolOutput, ToolSpec, Update, Value, channel, run,
};
use br_llm_messages::{Author, Conversation, Step, Text, ToolName, ToolResultBlock};
use serde_json::{Value as Json, json};

use common::{context, key, text_step, tool_call_step, user};

struct SkillModel {
    steps: Mutex<VecDeque<Step>>,
}

impl Model for SkillModel {
    fn complete<'a>(&'a self, request: Request, _sink: &'a dyn StreamSink) -> ModelFuture<'a> {
        let system = request
            .system
            .clone()
            .unwrap_or_else(|| "<none>".to_owned());
        let tools: Vec<&str> = request.tools.iter().map(|t| t.name.as_str()).collect();
        println!("  request system: {system:?}");
        println!("  request tools: {tools:?}");
        let step = self
            .steps
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        Box::pin(
            async move { step.ok_or_else(|| -> ModelError { "skill model exhausted".into() }) },
        )
    }
}

fn tool_spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: ToolName::new(name).expect("name"),
        description: format!("the {name} tool"),
        parameters: json!({ "type": "object" }),
    }
}

fn result(text: &str) -> ToolOutput {
    ToolOutput::text(vec![ToolResultBlock::text(
        Text::new(text).expect("non-empty"),
    )])
}

struct LoadSkill;
impl Tool for LoadSkill {
    fn spec(&self) -> ToolSpec {
        tool_spec("load_skill")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(&'a self, _a: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(result("skill 'math' loaded").with_updates(vec![
                Update::Append {
                    key: key("skill_prompts"),
                    value: Value::str("Use math carefully and show your work."),
                },
                Update::Append {
                    key: key("enabled"),
                    value: Value::str("special"),
                },
            ]))
        })
    }
}

struct UnloadSkill;
impl Tool for UnloadSkill {
    fn spec(&self) -> ToolSpec {
        tool_spec("unload_skill")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(&'a self, _a: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(result("skill 'math' unloaded").with_updates(vec![
                Update::Set {
                    key: key("skill_prompts"),
                    value: Value::list(Vec::new()),
                },
                Update::Set {
                    key: key("enabled"),
                    value: Value::list(vec![Value::str("load_skill"), Value::str("unload_skill")]),
                },
            ]))
        })
    }
}

struct SpecialTool;
impl Tool for SpecialTool {
    fn spec(&self) -> ToolSpec {
        tool_spec("special")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(&'a self, _a: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async { Ok(result("special capability used")) })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("skill_prompts"), Kind::list(Kind::Str))
        .state(key("enabled"), Kind::list(Kind::Str))
        .config(key("base"), Kind::Str)
        .build();

    let model: Arc<dyn Model> = Arc::new(SkillModel {
        steps: Mutex::new(
            vec![
                tool_call_step("c1", "load_skill", json!({ "skill": "math" })),
                tool_call_step("c2", "special", json!({})),
                tool_call_step("c3", "unload_skill", json!({ "skill": "math" })),
                text_step("All set."),
            ]
            .into_iter()
            .collect(),
        ),
    });

    let llm = LlmNode {
        key: key("chat"),
        author: Author::new("agent").expect("author"),
        model,
        system: vec![
            Source::Config(key("base")),
            Source::State(key("skill_prompts")),
        ],
        tools: vec![
            Arc::new(LoadSkill),
            Arc::new(UnloadSkill),
            Arc::new(SpecialTool),
        ],
        enabled: Some(key("enabled")),
        output: OutputMode::Text,
    };
    let react = ReactLoop {
        llm: NodeId::new("llm").expect("id"),
        tool_nodes: vec![(
            NodeId::new("tools").expect("id"),
            vec![
                Arc::new(LoadSkill),
                Arc::new(UnloadSkill),
                Arc::new(SpecialTool),
            ],
        )],
        after: Target::End(EndLabel::new("done").expect("label")),
    };
    let graph = react
        .add(
            GraphBuilder::new(schema.clone()).entry(NodeId::new("llm").expect("id")),
            llm,
        )
        .expect("partition")
        .build()
        .expect("build");

    let mut config_values = BTreeMap::new();
    config_values.insert(key("base"), Value::str("You are a capable agent."));
    let config = Config::new(&schema, config_values).expect("config");

    let mut chat = Conversation::new();
    chat.push_input(user("Solve a tricky problem, using a skill if helpful."));
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("skill_prompts"), Value::list(Vec::new()));
    values.insert(
        key("enabled"),
        Value::list(vec![Value::str("load_skill"), Value::str("unload_skill")]),
    );
    let state = State::new(schema.clone(), values).expect("state");

    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config, state, None, &context(), &mut inbox).await;
    match outcome {
        Ok(Outcome::Finished { end, .. }) => println!("skills finished on End({end})"),
        Ok(_) => println!("skills paused or cancelled"),
        Err(failure) => println!("skills failed: {}", failure.error),
    }
}
