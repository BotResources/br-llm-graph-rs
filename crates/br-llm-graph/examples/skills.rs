//! Skills on a ReAct agent: loading a skill adds its prompt to the system
//! text and enables its tool for the rest of the run, unloading takes both
//! back. The tools change the agent's state; the next model call reads it.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{OutputMode, ToolSpec};
use br_llm_graph::{
    Config, ConfigKey, Model, Outcome, Prompt, ReactAgent, Reads, State, StateKey, Tool,
    ToolFuture, ToolOutput, Update, Value, control, run,
};
use br_llm_messages::{Author, Conversation, Text, ToolResultBlock};
use serde_json::{Value as Json, json};

use common::tools::{text_parameter, tool_spec};
use common::{context, key, scripted, text_step, tool_call_step, user};

fn result(text: &str) -> ToolOutput {
    ToolOutput::text(vec![ToolResultBlock::text(
        Text::new(text).expect("non-empty"),
    )])
}

struct LoadSkill {
    spec: ToolSpec,
}
impl Tool for LoadSkill {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _a: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(result("skill 'math' loaded").with_updates(vec![
                Update::Set {
                    key: key("skill_prompt"),
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

struct UnloadSkill {
    spec: ToolSpec,
}
impl Tool for UnloadSkill {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _a: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(result("skill 'math' unloaded").with_updates(vec![
                Update::Set {
                    key: key("skill_prompt"),
                    value: Value::str(""),
                },
                Update::Set {
                    key: key("enabled"),
                    value: Value::list(vec![Value::str("load_skill"), Value::str("unload_skill")]),
                },
            ]))
        })
    }
}

struct SpecialTool {
    spec: ToolSpec,
}
impl Tool for SpecialTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _a: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async { Ok(result("special capability used")) })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = scripted(vec![
        tool_call_step("c1", "load_skill", json!({ "skill": "math" })),
        tool_call_step("c2", "special", json!({})),
        tool_call_step("c3", "unload_skill", json!({ "skill": "math" })),
        text_step("All set."),
    ]);
    let skill = || vec![text_parameter("skill", "the skill to load or unload")];

    // Inputs `opening`, `skill_prompt` (a string read into the system text)
    // and `enabled` (the tools offered); configuration `base`. The system
    // text is a function of the agent's context, which reads only the keys
    // declared with it.
    let mut reads = Reads::new();
    let base: ConfigKey<String> = reads.config("base").expect("key");
    let skill_prompt: StateKey<String> = reads.state("skill_prompt").expect("key");
    let system = Prompt::from_fn(reads, move |ctx| {
        Ok(format!(
            "{}\n\n{}",
            ctx.config(&base)?,
            ctx.get(&skill_prompt)?
        ))
    });
    let graph = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(client.clone()),
        system,
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![
            Arc::new(LoadSkill {
                spec: tool_spec("load_skill", "load a skill", skill()),
            }),
            Arc::new(UnloadSkill {
                spec: tool_spec("unload_skill", "unload a skill", skill()),
            }),
            Arc::new(SpecialTool {
                spec: tool_spec("special", "the capability of the math skill", Vec::new()),
            }),
        ],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: Some(key("enabled")),
        inputs: Vec::new(),
        thread_store: None,
    }
    .graph()
    .expect("agent graph");

    let mut config_values = BTreeMap::new();
    config_values.insert(key("base"), Value::str("You are a capable agent."));
    let config = Config::new(graph.schema(), config_values).expect("config");

    let mut chat = Conversation::new();
    chat.push_input(user("Solve a tricky problem, using a skill if helpful."))
        .expect("an input id unique in the conversation");
    let state = graph
        .start_state([
            (key("opening"), Value::conversation(chat)),
            (key("skill_prompt"), Value::str("")),
            (
                key("enabled"),
                Value::list(vec![Value::str("load_skill"), Value::str("unload_skill")]),
            ),
        ])
        .expect("start state");

    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config, state, None, &context(), &mut channel).await;
    for call in client.log() {
        let request = call.request();
        let system = request.system().map_or("<none>", Text::as_str);
        let tools: Vec<&str> = request
            .tools()
            .iter()
            .map(|tool| tool.name().as_str())
            .collect();
        println!(
            "  request {}: system {system:?}, tools {tools:?}",
            call.sequence()
        );
    }
    match outcome {
        Ok(Outcome::Finished { end, .. }) => println!("skills finished on End({end})"),
        Ok(_) => println!("skills paused or cancelled"),
        Err(failure) => println!("skills failed: {}", failure.error),
    }
}
