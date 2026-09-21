use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource,
};
use serde_json::json;

use crate::graph::{Always, Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::helpers::{pending_unsafe_calls, structured};
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::react::tool_node::ToolNode;
use crate::run::Outcome;
use crate::run::channel;
use crate::run::run;
use crate::state::{State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

fn state_with_call(step: Step) -> State {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("hi").unwrap())],
        )
        .unwrap(),
    );
    chat.push_turn(Turn::new(TurnId::new("t1").unwrap(), Some(author()), step))
        .unwrap();
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(key("enabled"), Value::list(Vec::new()));
    State::new(schema(), values).unwrap()
}

async fn run_tool_node(state: State, tools: Vec<Arc<dyn Tool>>) -> State {
    let graph = GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(
            nid("tools"),
            ToolNode {
                key: key("chat"),
                author: author(),
                tools,
            },
        )
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    state
}

fn first_result_is_error(state: &State) -> Option<bool> {
    let convo = state.conversation(&key("chat")).ok()?;
    for entry in convo.entries() {
        if let Entry::Turn(turn) = entry {
            for item in turn.items() {
                if let TurnItem::ToolResults(results) = item {
                    return results.results().first().map(|r| r.is_error);
                }
            }
        }
    }
    None
}

#[tokio::test]
async fn given_arguments_that_fail_to_parse_when_tool_runs_then_error_result() {
    let step = tool_call_step("c1", "strict", json!({ "n": "not a number" }));
    let state = run_tool_node(state_with_call(step), vec![Arc::new(StrictTool)]).await;
    assert_eq!(first_result_is_error(&state), Some(true));
}

#[tokio::test]
async fn given_tool_returning_updates_when_run_then_updates_applied() {
    let step = tool_call_step("c1", "tally", json!({}));
    let state = run_tool_node(state_with_call(step), vec![Arc::new(TallyTool)]).await;
    assert_eq!(state.list(&key("log")).unwrap().len(), 1);
    assert_eq!(first_result_is_error(&state), Some(false));
}

#[test]
fn given_structured_step_when_read_then_deserializes() {
    #[derive(serde::Deserialize)]
    struct Verdict {
        achieved: bool,
    }
    let step = structured_step(json!({ "achieved": true }));
    let verdict: Verdict = structured(&step).unwrap();
    assert!(verdict.achieved);
}

#[test]
fn given_text_only_step_when_structured_then_error() {
    #[derive(serde::Deserialize)]
    struct Verdict {
        #[allow(dead_code)]
        achieved: bool,
    }
    let step = text_step("no json here");
    assert!(structured::<Verdict>(&step).is_err());
}

#[test]
fn given_mixed_calls_when_pending_unsafe_then_only_unsafe_returned() {
    let step = Step::new(
        vec![
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c1").unwrap(),
                name: ToolName::new("write_file").unwrap(),
                arguments: json!({}),
            }),
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c2").unwrap(),
                name: ToolName::new("echo").unwrap(),
                arguments: json!({}),
            }),
        ],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let state = state_with_call(step);
    let registry: Vec<Arc<dyn Tool>> = vec![Arc::new(WriteFileTool::default()), Arc::new(EchoTool)];
    let convo = state.conversation(&key("chat")).unwrap();
    let unsafe_calls = pending_unsafe_calls(convo, &author(), &registry);
    assert_eq!(unsafe_calls.len(), 1);
    assert_eq!(unsafe_calls[0].name.as_str(), "write_file");
}
