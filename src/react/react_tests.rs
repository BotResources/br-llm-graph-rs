use std::sync::Arc;

use serde_json::json;

use crate::graph::{Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::OutputMode;
use crate::react::react_loop::ReactLoop;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::run::Outcome;
use crate::run::channel;
use crate::run::run;
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

fn llm_node(model: Arc<dyn crate::react::model::Model>, tools: Vec<Arc<dyn Tool>>) -> LlmNode {
    LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        tools,
        enabled: None,
        output: OutputMode::Text,
    }
}

#[tokio::test]
async fn given_a_full_react_loop_when_run_then_finishes_with_turn_of_three_items() {
    let model = Arc::new(ScriptedModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("all done"),
    ]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap();

    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), seeded_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, end } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "done");
    let convo = state.conversation(&key("chat")).unwrap();
    let turn = convo
        .entries()
        .iter()
        .find_map(|entry| match entry {
            br_llm_messages::Entry::Turn(turn) => Some(turn),
            br_llm_messages::Entry::UserInput(_) => None,
        })
        .unwrap();
    assert_eq!(turn.items().len(), 3);
}

#[tokio::test]
async fn given_two_tool_nodes_with_parallel_calls_when_run_then_both_run() {
    let model = Arc::new(ScriptedModel::new(vec![
        two_call_step(),
        text_step("done both"),
    ]));
    let llm = llm_node(model, vec![Arc::new(EchoTool), Arc::new(TallyTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![
            (nid("echo_node"), vec![Arc::new(EchoTool)]),
            (nid("tally_node"), vec![Arc::new(TallyTool)]),
        ],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), seeded_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.list(&key("log")).unwrap().len(), 1);
}

fn two_call_step() -> br_llm_messages::Step {
    use br_llm_messages::{AssistantBlock, Step, StopReason, ToolCall, ToolCallId, ToolName};
    Step::new(
        vec![
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c1").unwrap(),
                name: ToolName::new("echo").unwrap(),
                arguments: json!({}),
            }),
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c2").unwrap(),
                name: ToolName::new("tally").unwrap(),
                arguments: json!({}),
            }),
        ],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap()
}

#[test]
fn given_tool_not_covered_when_react_loop_added_then_refused() {
    let model = Arc::new(ScriptedModel::new(vec![text_step("x")]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), Vec::new())],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotCovered { .. })
    ));
}

#[test]
fn given_tool_covered_twice_when_react_loop_added_then_refused() {
    let model = Arc::new(ScriptedModel::new(vec![text_step("x")]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![
            (nid("t1"), vec![Arc::new(EchoTool)]),
            (nid("t2"), vec![Arc::new(EchoTool)]),
        ],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolCoveredTwice { .. })
    ));
}

#[test]
fn given_tool_node_tool_not_declared_when_react_loop_added_then_refused() {
    let model = Arc::new(ScriptedModel::new(vec![text_step("x")]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool), Arc::new(TallyTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotDeclared { .. })
    ));
}

#[tokio::test]
async fn given_model_calls_unowned_tool_when_run_then_pending_unsatisfiable() {
    let model = Arc::new(ScriptedModel::new(vec![tool_call_step(
        "c1",
        "ghost",
        json!({}),
    )]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), seeded_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::PendingToolUnsatisfiable { .. }
    ));
}

#[tokio::test]
async fn given_enabled_filter_when_llm_runs_then_only_enabled_tools_declared() {
    use crate::state::Value;
    let model = Arc::new(RecordingModel::new());
    let recording = model.clone();
    let llm = LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        tools: vec![Arc::new(EchoTool), Arc::new(TallyTool)],
        enabled: Some(key("enabled")),
        output: OutputMode::Text,
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            crate::graph::Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();

    let mut state = seeded_state();
    state
        .apply_batch(&[crate::update::Update::Set {
            key: key("enabled"),
            value: Value::list(vec![Value::str("echo")]),
        }])
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let seen = recording.tools_seen.lock().unwrap().clone();
    assert_eq!(seen, vec!["echo".to_owned()]);
}

#[tokio::test]
async fn given_multiple_system_sources_when_llm_runs_then_joined_by_blank_line() {
    let model = Arc::new(RecordingModel::new());
    let recording = model.clone();
    let llm = LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base")), Source::State(key("enabled"))],
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Text,
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            crate::graph::Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), seeded_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let system = recording.system_seen.lock().unwrap().clone().unwrap();
    assert_eq!(system, "You are a helpful agent.\n\necho\n\ntally");
}

#[tokio::test]
async fn given_generator_critic_graph_when_run_then_validates_after_one_iteration() {
    use std::collections::BTreeMap;

    use br_llm_messages::{Conversation, Text, UserBlock, UserInput, UserSource};

    use crate::graph::{Always, FnEdge, FnNode, NodeFuture};
    use crate::react::helpers::{complete, structured, wire};
    use crate::react::model::{Model, Request};
    use crate::state::{Config, Kind, Schema, State, Value};
    use crate::update::Update;
    use crate::value::Key;

    #[derive(serde::Deserialize)]
    struct Verdict {
        validated: bool,
    }

    fn k(name: &str) -> Key {
        Key::new(name).unwrap()
    }

    let schema = Schema::builder()
        .state(k("chat"), Kind::Conversation)
        .state(k("is_validated"), Kind::Bool)
        .state(k("num_iter"), Kind::Int)
        .config(k("base"), Kind::Str)
        .build();

    let generator = LlmNode {
        key: k("chat"),
        author: author(),
        model: Arc::new(ScriptedModel::new(vec![
            text_step("first attempt"),
            text_step("revised attempt"),
        ])),
        system: vec![Source::Config(k("base"))],
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Text,
    };

    let critic_model: Arc<dyn Model> = Arc::new(ScriptedModel::new(vec![
        structured_step(json!({ "validated": false })),
        structured_step(json!({ "validated": true })),
    ]));
    let critic_author = author();
    let critic = FnNode::new(
        move |state: &State, _config: &Config, ctx: &Context| -> NodeFuture<'_> {
            let model = critic_model.clone();
            let author = critic_author.clone();
            Box::pin(async move {
                let messages = wire(state.conversation(&k("chat"))?, &author)?;
                let request = Request {
                    system: None,
                    messages,
                    tools: Vec::new(),
                    output: OutputMode::Structured { schema: json!({}) },
                };
                let step = complete(model.as_ref(), request, ctx, &k("chat")).await?;
                let verdict: Verdict = structured(&step)?;
                let mut updates = vec![Update::Set {
                    key: k("is_validated"),
                    value: Value::bool(verdict.validated),
                }];
                if !verdict.validated {
                    let next = state.int(&k("num_iter"))? + 1;
                    updates.push(Update::Set {
                        key: k("num_iter"),
                        value: Value::int(next),
                    });
                    let instruction = UserInput::new(
                        UserSource::runtime("critic")?,
                        None,
                        vec![UserBlock::text(Text::new("revise")?)],
                    )?;
                    updates.push(Update::Input {
                        key: k("chat"),
                        input: instruction,
                    });
                }
                Ok(updates)
            })
        },
    );

    let critic_edge = FnEdge::new(|state: &State, _config: &Config| {
        if state.bool(&k("is_validated"))? {
            Ok(vec![Target::End(EndLabel::new("validated").unwrap())])
        } else if state.int(&k("num_iter"))? >= 3 {
            Ok(vec![Target::End(EndLabel::new("gave_up").unwrap())])
        } else {
            Ok(vec![Target::Node(nid("generator"))])
        }
    });

    let graph = GraphBuilder::new(schema.clone())
        .entry(nid("generator"))
        .node(nid("generator"), generator)
        .node(nid("critic"), critic)
        .edge(nid("generator"), Always(Target::Node(nid("critic"))))
        .edge(nid("critic"), critic_edge)
        .build()
        .unwrap();

    let mut cfg_values = BTreeMap::new();
    cfg_values.insert(k("base"), Value::str("answer well"));
    let config = Config::new(&schema, cfg_values).unwrap();

    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("explain supersteps").unwrap())],
        )
        .unwrap(),
    );
    let mut values = BTreeMap::new();
    values.insert(k("chat"), Value::conversation(chat));
    values.insert(k("is_validated"), Value::bool(false));
    values.insert(k("num_iter"), Value::int(0));
    let state = State::new(schema, values).unwrap();

    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config, state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, end } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "validated");
    assert!(state.bool(&k("is_validated")).unwrap());
    assert_eq!(state.int(&k("num_iter")).unwrap(), 1);
}

#[tokio::test]
async fn given_background_task_when_completion_arrives_then_relaunch_finishes() {
    use std::collections::BTreeMap;

    use br_llm_messages::{Conversation, Entry, Text, UserBlock, UserInput, UserSource};
    use serde_json::Value as Json;

    use crate::graph::{FnEdge, FnNode, NodeFuture};
    use crate::react::model::ToolSpec;
    use crate::react::tool::{ToolFuture, ToolOutput};
    use crate::session::Session;
    use crate::state::{Config, Kind, Schema, State, Value};
    use crate::update::Update;
    use crate::value::Key;

    fn k(name: &str) -> Key {
        Key::new(name).unwrap()
    }

    struct StartTask {
        pending: Key,
    }

    impl Tool for StartTask {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: br_llm_messages::ToolName::new("start_task").unwrap(),
                description: "start a background task".to_owned(),
                parameters: json!({ "type": "object" }),
            }
        }
        fn safe(&self) -> bool {
            true
        }
        fn call<'a>(
            &'a self,
            _arguments: Json,
            _state: &'a State,
            _config: &'a Config,
        ) -> ToolFuture<'a> {
            let pending = self.pending.clone();
            Box::pin(async move {
                Ok(
                    ToolOutput::text(Vec::new()).with_updates(vec![Update::Append {
                        key: pending,
                        value: Value::str("task-1"),
                    }]),
                )
            })
        }
    }

    let schema = Schema::builder()
        .state(k("chat"), Kind::Conversation)
        .state(k("pending_tasks"), Kind::list(Kind::Str))
        .config(k("base"), Kind::Str)
        .build();

    let model = Arc::new(ScriptedModel::new(vec![
        tool_call_step("c1", "start_task", json!({})),
        text_step("task started, waiting"),
        text_step("the task is complete"),
    ]));
    let start_task = Arc::new(StartTask {
        pending: k("pending_tasks"),
    });
    let llm = LlmNode {
        key: k("chat"),
        author: author(),
        model,
        system: vec![Source::Config(k("base"))],
        tools: vec![start_task.clone()],
        enabled: None,
        output: OutputMode::Text,
    };
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![start_task])],
        after: Target::Node(nid("gate")),
    };
    let gate = FnNode::new(|_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async { Ok(Vec::new()) })
    });
    let gate_edge = FnEdge::new(|state: &State, _config: &Config| {
        let completed = state
            .conversation(&k("chat"))?
            .entries()
            .iter()
            .any(|entry| {
                matches!(
                    entry,
                    Entry::UserInput(input) if matches!(input.source(), UserSource::Runtime { .. })
                )
            });
        if completed {
            Ok(vec![Target::End(EndLabel::new("done").unwrap())])
        } else {
            Ok(vec![Target::End(EndLabel::new("waiting").unwrap())])
        }
    });
    let graph = Arc::new(
        react
            .add(GraphBuilder::new(schema.clone()).entry(nid("llm")), llm)
            .unwrap()
            .node(nid("gate"), gate)
            .edge(nid("gate"), gate_edge)
            .build()
            .unwrap(),
    );

    let mut cfg_values = BTreeMap::new();
    cfg_values.insert(k("base"), Value::str("kick off work then wait"));
    let config = Config::new(&schema, cfg_values).unwrap();

    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("run the long job").unwrap())],
        )
        .unwrap(),
    );
    let mut values = BTreeMap::new();
    values.insert(k("chat"), Value::conversation(chat));
    values.insert(k("pending_tasks"), Value::list(Vec::new()));
    let state = State::new(schema, values).unwrap();

    let mut session = Session::new(graph, config, state, ctx());
    let sender = session.sender();

    let first = session.run_once().await.unwrap();
    let Outcome::Finished { end, .. } = first else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "waiting");
    assert_eq!(session.state().list(&k("pending_tasks")).unwrap().len(), 1);

    sender.send(
        k("chat"),
        UserInput::new(
            UserSource::runtime("task_done").unwrap(),
            None,
            vec![UserBlock::text(Text::new("task-1 finished").unwrap())],
        )
        .unwrap(),
    );
    let second = session.run_once().await.unwrap();
    let Outcome::Finished { end, .. } = second else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "done");
}
