//! Skills on a ReAct agent: loading a skill adds its prompt to the system
//! text and enables its tool for the rest of the run, unloading takes both
//! back. The tools change the agent's state through typed keys; the next
//! model call reads it. The agent works in memory: without a thread store,
//! its conversation lives for the current call only.

#[allow(dead_code)]
mod common;

use std::sync::Arc;

use br_llm_clients_core::{OutputMode, ToolSpec};
use br_llm_graph::{
    Config, ConfigKey, GraphError, Model, Outcome, Prompt, ReactAgent, Reads, State, StateKey,
    Tool, ToolContext, ToolFuture, ToolOutput, Value, control, run,
};
use br_llm_messages::{Author, Conversation, Text, ToolResultBlock};
use serde_json::{Value as Json, json};

use common::tools::{text_parameter, tool_spec};
use common::{context, opening, scripted, state_key, text_step, tool_call_step, user};

fn result(text: &str) -> ToolOutput {
    ToolOutput::text(vec![ToolResultBlock::text(
        Text::new(text).expect("non-empty"),
    )])
}

/// The keys the skill tools write: the skill's prompt and the tools offered.
#[derive(Clone)]
struct SkillKeys {
    prompt: StateKey<String>,
    enabled: StateKey<Vec<String>>,
}

struct LoadSkill {
    spec: ToolSpec,
    keys: SkillKeys,
}
impl Tool for LoadSkill {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _a: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
        let updates = (|| {
            Ok::<_, GraphError>(vec![
                self.keys
                    .prompt
                    .set("Use math carefully and show your work.")?,
                self.keys.enabled.append("special")?,
            ])
        })();
        Box::pin(async move { Ok(result("skill 'math' loaded").with_updates(updates?)) })
    }
}

struct UnloadSkill {
    spec: ToolSpec,
    keys: SkillKeys,
}
impl Tool for UnloadSkill {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _a: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
        let updates = (|| {
            Ok::<_, GraphError>(vec![
                self.keys.prompt.set("")?,
                self.keys.enabled.set(base_tools())?,
            ])
        })();
        Box::pin(async move { Ok(result("skill 'math' unloaded").with_updates(updates?)) })
    }
}

struct SpecialTool {
    spec: ToolSpec,
}
impl Tool for SpecialTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _a: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async { Ok(result("special capability used")) })
    }
}

/// The tools offered without a skill.
fn base_tools() -> Vec<String> {
    vec!["load_skill".to_owned(), "unload_skill".to_owned()]
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
    let keys = SkillKeys {
        prompt: reads.state("skill_prompt").expect("key"),
        enabled: state_key("enabled"),
    };
    let (read_base, read_prompt) = (base.clone(), keys.prompt.clone());
    let system = Prompt::from_fn(reads, move |ctx| {
        Ok(format!(
            "{}\n\n{}",
            ctx.config(&read_base)?,
            ctx.get(&read_prompt)?
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
                keys: keys.clone(),
            }),
            Arc::new(UnloadSkill {
                spec: tool_spec("unload_skill", "unload a skill", skill()),
                keys: keys.clone(),
            }),
            Arc::new(SpecialTool {
                spec: tool_spec("special", "the capability of the math skill", Vec::new()),
            }),
        ],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: Some(keys.enabled.key().clone()),
        inputs: Vec::new(),
        thread_store: None,
    }
    .graph()
    .expect("agent graph");

    let config = Config::new(
        graph.schema(),
        [(base.key().clone(), Value::str("You are a capable agent."))].into(),
    )
    .expect("config");
    let ask = opening([user("Solve a tricky problem, using a skill if helpful.")]);
    let state = graph
        .start_state([
            state_key::<Conversation>("opening")
                .given(ask)
                .expect("opening"),
            keys.prompt.given("").expect("no skill prompt"),
            keys.enabled.given(base_tools()).expect("the base tools"),
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
