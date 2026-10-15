use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{Client, OutputMode, ToolSpec};
use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource,
};
use serde_json::json;

use crate::graph::{Always, Context, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::helpers::{pending_calls, structured};
use crate::react::llm_node::LlmNode;
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::react::tool_gate::SharedGates;
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

/// A tool node of the agent on `chat`, with its own gates.
fn tool_node(
    tools: Vec<Arc<dyn Tool>>,
    max_concurrency: Option<Limit>,
    enabled: Option<crate::value::Key>,
) -> ToolNode {
    ToolNode {
        key: key("chat"),
        author: author(),
        tools,
        max_concurrency,
        enabled,
        gates: Arc::new(SharedGates::default()),
    }
}

/// A model-call step of the agent on `chat`, without tools.
fn llm_node(client: Arc<dyn Client>, system: Vec<Source>) -> LlmNode {
    LlmNode {
        key: key("chat"),
        author: author(),
        client,
        system,
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Plain,
    }
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
        .node(nid("tools"), tool_node(tools, None, None))
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

#[tokio::test]
async fn given_a_tool_node_with_an_enabled_key_when_calls_name_a_tool_it_does_not_hold_then_that_call_is_refused_unrun()
 {
    let writer = Arc::new(WriteFileTool::default());
    let mut state = state_with_call(calls_step(&["write_file", "echo"]));
    state
        .apply_batch(&[crate::update::Update::Set {
            key: key("enabled"),
            value: Value::list(vec![Value::str("echo")]),
        }])
        .unwrap();
    let graph = GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(
            nid("tools"),
            tool_node(
                vec![Arc::new(EchoTool), writer.clone()],
                None,
                Some(key("enabled")),
            ),
        )
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let Outcome::Finished { state, .. } = run(&graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap()
    else {
        panic!("expected finished");
    };
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    assert_eq!(
        results_of(turn),
        [
            (
                "write_file".to_owned(),
                true,
                "Tool write_file is not enabled for this run: this call was not executed."
                    .to_owned()
            ),
            ("echo".to_owned(), false, "echoed".to_owned()),
        ]
    );
    assert_eq!(writer.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn given_an_enabled_key_that_is_not_a_list_of_strings_when_built_then_refused() {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    for name in ["chat", "missing"] {
        let mut llm = llm_node(client.clone(), Vec::new());
        llm.tools = vec![Arc::new(EchoTool)];
        llm.enabled = Some(key(name));
        let built = GraphBuilder::new(schema())
            .entry(nid("llm"))
            .node(nid("llm"), llm)
            .edge(
                nid("llm"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build();
        assert!(matches!(
            crate::testkit::refusal(built),
            crate::error::GraphError::EnabledKeyMismatch { key: found } if found == key(name)
        ));
        let built = GraphBuilder::new(schema())
            .entry(nid("tools"))
            .node(
                nid("tools"),
                tool_node(vec![Arc::new(EchoTool)], None, Some(key(name))),
            )
            .edge(
                nid("tools"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build();
        assert!(matches!(
            crate::testkit::refusal(built),
            crate::error::GraphError::EnabledKeyMismatch { .. }
        ));
    }
}

/// The build of a graph whose single node is `node`.
fn built_alone(node: impl crate::graph::Node + 'static) -> crate::error::GraphError {
    crate::testkit::refusal(
        GraphBuilder::new(schema())
            .entry(nid("only"))
            .node(nid("only"), node)
            .edge(
                nid("only"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build(),
    )
}

#[test]
fn given_a_tool_concurrency_on_a_key_that_is_not_an_int_configuration_key_when_built_then_refused()
{
    for name in ["base", "missing"] {
        let tools = tool_node(
            vec![Arc::new(EchoTool)],
            Some(Limit::Config(key(name))),
            None,
        );
        assert!(matches!(
            built_alone(tools),
            crate::error::GraphError::LimitKeyMismatch { key: found } if found == key(name)
        ));
    }
}

#[test]
fn given_a_system_source_on_a_key_that_holds_no_text_when_built_then_refused() {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    let llm = |source: Source| llm_node(client.clone(), vec![Source::Config(key("base")), source]);
    for (source, name) in [
        (Source::State(key("chat")), "chat"),
        (Source::State(key("missing")), "missing"),
        (Source::State(key("base")), "base"),
        (Source::Config(key("log")), "log"),
        (Source::Config(key("missing")), "missing"),
    ] {
        assert!(matches!(
            built_alone(llm(source)),
            crate::error::GraphError::SourceKeyMismatch { key: found } if found == key(name)
        ));
    }
    let accepted = GraphBuilder::new(schema())
        .entry(nid("only"))
        .node(nid("only"), llm(Source::State(key("log"))))
        .edge(
            nid("only"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build();
    assert!(accepted.is_ok());
}

#[tokio::test]
async fn given_llm_invoked_with_awaiting_results_turn_when_run_then_fails_closed() {
    let state = state_with_call(tool_call_step("c1", "echo", json!({})));
    let client = mock(vec![text_step("fresh")]);
    let mut llm = llm_node(Arc::new(client.clone()), vec![Source::Config(key("base"))]);
    llm.tools = vec![Arc::new(EchoTool)];
    let graph = GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        returned(&failure.error),
        crate::error::GraphError::Message(br_llm_messages::MessageError::TurnAwaitingResults)
    ));
    assert_eq!(client.calls(), 0, "the render refuses before any call");
    assert_eq!(
        failure
            .checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        2
    );
}

#[tokio::test]
async fn given_two_tools_set_same_key_when_run_then_node_fails_and_state_unchanged() {
    use crate::react::tool::{ToolFuture, ToolOutput};
    use crate::state::Config;
    use crate::update::Update;

    struct SetLog {
        spec: ToolSpec,
    }
    impl Tool for SetLog {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        fn call<'a>(
            &'a self,
            _arguments: serde_json::Value,
            _state: &'a State,
            _config: &'a Config,
        ) -> ToolFuture<'a> {
            Box::pin(async {
                Ok(ToolOutput::text(Vec::new()).with_updates(vec![Update::Set {
                    key: key("log"),
                    value: Value::list(vec![Value::str("x")]),
                }]))
            })
        }
    }

    let step = Step::new(
        vec![
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c1").unwrap(),
                name: ToolName::new("seta").unwrap(),
                arguments: json!({}),
            }),
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c2").unwrap(),
                name: ToolName::new("setb").unwrap(),
                arguments: json!({}),
            }),
        ],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let graph = GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(
            nid("tools"),
            tool_node(
                vec![
                    Arc::new(SetLog { spec: spec("seta") }),
                    Arc::new(SetLog { spec: spec("setb") }),
                ],
                None,
                None,
            ),
        )
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(
        &graph,
        &config(),
        state_with_call(step),
        None,
        &ctx(),
        &mut inbox,
    )
    .await
    .err()
    .unwrap();
    match failure.error {
        crate::error::GraphError::NodeFailed { source, .. } => {
            assert!(matches!(source, crate::error::NodeFault::Refused(_)));
        }
        other => panic!("expected NodeFailed, got {other}"),
    }
    assert!(
        failure
            .checkpoint
            .state
            .list(&key("log"))
            .unwrap()
            .is_empty()
    );
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
fn given_a_turn_awaiting_results_when_pending_calls_read_then_the_unanswered_calls_in_call_order() {
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
    let names = |state: &State| -> Vec<String> {
        pending_calls(state, &key("chat"), &author())
            .unwrap()
            .iter()
            .map(|call| call.name.as_str().to_owned())
            .collect()
    };
    assert_eq!(names(&state), ["write_file", "echo"]);

    let mut answered = state.clone();
    answered
        .apply_batch(&[crate::update::Update::PushResult {
            key: key("chat"),
            turn: TurnId::new("t1").unwrap(),
            result: br_llm_messages::ToolResult::new(
                ToolCallId::new("c1").unwrap(),
                ToolName::new("write_file").unwrap(),
                Vec::new(),
                false,
            ),
        }])
        .unwrap();
    assert_eq!(names(&answered), ["echo"]);
    assert!(
        pending_calls(&seeded_state(), &key("chat"), &author())
            .unwrap()
            .is_empty(),
        "an author without a turn has no pending call"
    );
}
