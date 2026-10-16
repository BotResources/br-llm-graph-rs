use std::sync::Arc;

use serde_json::json;

use crate::graph::{Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::given::Model;
use crate::react::llm_node::LlmNode;
use crate::react::react_agent::ReactAgent;
use crate::react::react_loop::ReactLoop;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::run::Outcome;
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};
use br_llm_clients_core::OutputMode;

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

#[tokio::test]
async fn given_background_task_when_completion_arrives_then_relaunch_finishes() {
    use std::collections::BTreeMap;

    use br_llm_messages::{Conversation, Entry, InputId, Text, UserBlock, UserInput, UserSource};
    use serde_json::Value as Json;

    use crate::graph::{FnEdge, FnNode, NodeFuture};
    use crate::react::tool::{ToolFuture, ToolOutput};
    use crate::session::Session;
    use crate::state::{Config, Kind, Schema, State, Value};
    use crate::update::Update;
    use crate::value::Key;
    use br_llm_clients_core::ToolSpec;

    fn k(name: &str) -> Key {
        Key::new(name).unwrap()
    }

    struct StartTask {
        spec: ToolSpec,
        pending: Key,
    }

    impl Tool for StartTask {
        fn spec(&self) -> &ToolSpec {
            &self.spec
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
        .state_by_name(k("chat"), Kind::Conversation)
        .state_by_name(k(TURN_RANK), Kind::Int)
        .state_by_name(k("pending_tasks"), Kind::list(Kind::Str))
        .config_by_name(k("base"), Kind::Str)
        .build();

    let client = Arc::new(mock(vec![
        tool_call_step("c1", "start_task", json!({})),
        text_step("task started, waiting"),
        text_step("the task is complete"),
    ]));
    let start_task = Arc::new(StartTask {
        spec: spec("start_task"),
        pending: k("pending_tasks"),
    });
    let llm = LlmNode::<ReactAgent> {
        key: k("chat"),
        author: author(),
        model: Model::client(client),
        system: base(),
        history: None,
        tools: vec![start_task.clone()],
        enabled: None,
        output_mode: OutputMode::Plain,
        tool_concurrency: None,
        round_limit: None,
        call: in_memory_call(),
        seat: None,
    };
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![start_task])],
        after: Target::Node(nid("gate")),
        tool_concurrency: None,
        round_limit: None,
        thread: None,
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
            InputId::new("i-run-the-long-job").unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("run the long job").unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
    let mut values = BTreeMap::new();
    values.insert(k("chat"), Value::conversation(chat));
    values.insert(k("pending_tasks"), Value::list(Vec::new()));
    values.insert(k(TURN_RANK), first_turn());
    let state = State::new(schema, values).unwrap();

    let mut session = Session::new(graph, config, state, ctx()).unwrap();
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
            InputId::new("i-task-1-finished").unwrap(),
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
