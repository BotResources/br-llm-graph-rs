//! A hook of the host's guarding a tool of a ReAct agent: each call of
//! `write_file` is judged before the tool runs. A path outside `notes/` is
//! refused with a reason the model reads as an error result; the turn goes
//! on and the model answers. The graph ships no hook: judging a call is the
//! host's policy.

#[allow(dead_code)]
mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use br_llm_clients_core::{OutputMode, ToolSpec};
use br_llm_graph::{
    Config, Hook, HookFuture, Judgment, Model, Outcome, Prompt, ReactAgent, State, Tool,
    ToolFuture, Value, control, run,
};
use br_llm_messages::{Author, Conversation, ToolCallId};
use serde_json::{Value as Json, json};

use common::tools::WriteFileTool;
use common::{context, key, scripted_as_written, text_step, tool_call_step, user};

/// Accepts a write under `notes/`, refuses any other.
struct NotesOnly;

impl Hook for NotesOnly {
    fn judge<'a>(
        &'a self,
        id: &'a ToolCallId,
        arguments: &'a Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> HookFuture<'a> {
        Box::pin(async move {
            let path = arguments["path"].as_str().unwrap_or_default();
            println!("  hook: call {id} writes {path:?}");
            if path.starts_with("notes/") && !path.contains("..") {
                Ok(Judgment::Accept)
            } else {
                Ok(Judgment::Refuse {
                    reason: format!("{path} is outside notes/: only notes may be written"),
                })
            }
        })
    }
}

/// The host's file tool, guarded by its hook.
struct GuardedWrite {
    tool: WriteFileTool,
    hook: NotesOnly,
}

impl Tool for GuardedWrite {
    fn spec(&self) -> &ToolSpec {
        self.tool.spec()
    }

    fn hook(&self) -> Option<&dyn Hook> {
        Some(&self.hook)
    }

    fn call<'a>(&'a self, arguments: Json, state: &'a State, config: &'a Config) -> ToolFuture<'a> {
        self.tool.call(arguments, state, config)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // The ids `c1` and `c2` reach the hook as the client gives them.
    let client = scripted_as_written(vec![
        tool_call_step(
            "c1",
            "write_file",
            json!({ "path": "/etc/hosts", "content": "x" }),
        ),
        tool_call_step(
            "c2",
            "write_file",
            json!({ "path": "notes/tides.md", "content": "The moon pulls." }),
        ),
        text_step("Saved to notes/tides.md; /etc/hosts was refused."),
    ]);
    let write = Arc::new(GuardedWrite {
        tool: WriteFileTool::new(),
        hook: NotesOnly,
    });

    // Input `opening`; configuration none: the system text is fixed.
    let graph = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(client),
        system: Prompt::text("Save what the user asks for."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![write.clone()],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
        thread_store: None,
    }
    .graph()
    .expect("agent graph");

    let mut chat = Conversation::new();
    chat.push_input(user("Save a note on tides."))
        .expect("an input id unique in the conversation");
    let state = graph
        .start_state([(key("opening"), Value::conversation(chat))])
        .expect("start state");
    let config = Config::new(graph.schema(), Default::default()).expect("config");

    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!(
                "tool_hook: {} (the tool ran {} time)",
                state.str(&key("answer")).expect("answer"),
                write.tool.calls.load(Ordering::SeqCst)
            );
        }
        Ok(_) => println!("tool_hook paused or cancelled"),
        Err(failure) => println!("tool_hook failed: {}", failure.error),
    }
}
