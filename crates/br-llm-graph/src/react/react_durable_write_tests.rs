//! What a durable ReAct agent writes to its thread: every step, tool results
//! item and turn end there first, then in `conversation`; one open message
//! per turn across its rounds; a round's results as one item whatever the
//! split; the round limit's writes; an error of the store as `Thread`.

use std::sync::{Arc, Mutex};

use br_llm_messages::{StopReason, TurnId, TurnItem};
use serde_json::json;

use crate::observe::Observer;
use crate::origin::Origin;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::round_limit::OnLimit;
use crate::react::test_support::*;
use crate::react::{MessageStatus, ThreadStore};
use crate::testkit::{Op, StoreError, TestStore};
use crate::update::Update;

/// At each update applied to the agent's `conversation`, what the thread
/// holds of the turn it names: its items and its status.
struct ThreadFirst {
    store: Arc<TestStore>,
    seen: Mutex<Vec<(&'static str, usize, MessageStatus)>>,
}

impl Observer for ThreadFirst {
    fn applied(&self, _origin: &Origin, update: &Update) {
        let (kind, turn) = match update {
            Update::PushTurn { turn, .. } => ("turn", turn.id().clone()),
            Update::PushStep { turn, .. } => ("step", turn.clone()),
            Update::PushResult { turn, .. } => ("result", turn.clone()),
            _ => return,
        };
        let view = self.store.view(&nth_thread(1)).unwrap();
        let items = view
            .conversation
            .turn(&turn)
            .map_or(0, |turn| turn.items().len());
        let status = view.message(turn.as_str()).unwrap().status;
        self.seen.lock().unwrap().push((kind, items, status));
    }
}

#[tokio::test]
async fn given_a_tool_round_when_the_agent_runs_then_each_write_reaches_the_thread_before_the_conversation_in_one_open_turn()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("echo said echoed"),
    ]);
    let graph = durable(&client, &store).graph().unwrap();
    let observer = Arc::new(ThreadFirst {
        store: store.clone(),
        seen: Mutex::default(),
    });
    let host = Host::observed_by(observer.clone());

    let done = finished(call_durable(&host, &graph, opening(&[said("i1", "hi")]), "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "echo said echoed");
    assert_eq!(
        *observer.seen.lock().unwrap(),
        [
            ("turn", 1, MessageStatus::Streaming),
            ("result", 2, MessageStatus::Streaming),
            ("step", 3, MessageStatus::Completed),
        ]
    );
    let mut lines = writes(&store);
    lines.dedup();
    assert_eq!(
        lines[2..],
        [
            "open t2",
            "event t2",
            "finish t2",
            "results t2@1",
            "event t2",
            "finish t2",
            "end t2 Completed"
        ]
    );
}

#[tokio::test]
async fn given_tools_split_between_nodes_when_a_round_calls_them_all_then_the_thread_takes_one_item_in_node_order()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![calls_step(&["tally", "echo"]), text_step("both ran")]);
    let echo: Arc<dyn crate::react::Tool> = Arc::new(EchoTool);
    let tally: Arc<dyn crate::react::Tool> = Arc::new(TallyTool);
    let graph = ReactAgent {
        tools: vec![echo.clone(), tally.clone()],
        tool_nodes: vec![(nid("echoes"), vec![echo]), (nid("tallies"), vec![tally])],
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();

    let done =
        finished(call_durable(&Host::new(), &graph, opening(&[said("i1", "go")]), "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "both ran");
    assert_eq!(done.list(&key("log")).unwrap().len(), 1);
    let pushes: Vec<String> = writes(&store)
        .into_iter()
        .filter(|line| line.starts_with("results"))
        .collect();
    assert_eq!(pushes, ["results t2@1"]);
    let view = store.view(&nth_thread(1)).unwrap();
    let turn = view.conversation.turn(&TurnId::new("t2").unwrap()).unwrap();
    assert_eq!(
        results_of(turn),
        [
            ("echo".to_owned(), false, "echoed".to_owned()),
            ("tally".to_owned(), false, "tallied".to_owned()),
        ]
    );
    assert_eq!(results_of(turn), results_of(&last_turn(&done)));
}

#[tokio::test]
async fn given_the_round_limit_ends_a_durable_turn_when_it_is_reached_then_its_refusals_and_closing_step_are_written_and_the_turn_completed()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let graph = ReactAgent {
        thread_store: Some(store.clone()),
        inputs: vec![(
            key("log"),
            crate::state::Kind::list(crate::state::Kind::Str),
        )],
        ..limited(
            &client,
            1,
            OnLimit::End {
                node: nid("limit"),
                flag: key("limit_reached"),
            },
        )
    }
    .graph()
    .unwrap();

    let done =
        finished(call_durable(&Host::new(), &graph, opening(&[said("i1", "go")]), "", "r1").await);

    assert!(done.bool(&key("limit_reached")).unwrap());
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Completed);
    let turn = view.conversation.turn(&TurnId::new("t2").unwrap()).unwrap();
    assert_eq!(turn.items().len(), 5);
    let Some(TurnItem::Step(closing)) = turn.items().last() else {
        panic!("expected the closing step");
    };
    assert_eq!(
        closing.stop_reason(),
        &StopReason::Other {
            reason: "tool_round_limit".to_owned()
        }
    );
    let held = last_turn(&done);
    assert_eq!(turn.items().len(), held.items().len());
    assert_eq!(results_of(turn), results_of(&held));
    assert_eq!(
        results_of(turn)[1].2,
        "Tool round limit reached (1): this call was not executed. Reply without calling tools."
    );
}

#[tokio::test]
async fn given_the_round_limit_continues_a_durable_turn_when_it_is_reached_then_its_refusals_are_written_and_the_turn_goes_on()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("without tools"),
    ]);
    let graph = ReactAgent {
        thread_store: Some(store.clone()),
        inputs: vec![(
            key("log"),
            crate::state::Kind::list(crate::state::Kind::Str),
        )],
        ..limited(
            &client,
            1,
            OnLimit::Continue {
                node: nid("limit"),
                flag: key("limit_reached"),
            },
        )
    }
    .graph()
    .unwrap();

    let done =
        finished(call_durable(&Host::new(), &graph, opening(&[said("i1", "go")]), "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "without tools");
    let pushes: Vec<String> = writes(&store)
        .into_iter()
        .filter(|line| line.starts_with("results") || line.starts_with("open"))
        .collect();
    assert_eq!(pushes, ["open t2", "results t2@1", "results t2@3"]);
    let view = store.view(&nth_thread(1)).unwrap();
    let turn = view.conversation.turn(&TurnId::new("t2").unwrap()).unwrap();
    assert_eq!(turn.items().len(), 5);
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Completed);
}

/// The failure of a run of the durable agent whose store fails its first
/// call of `op`: the node it fails and the store's error.
async fn failing_on(op: Op) -> (String, StoreError) {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done"),
    ]);
    let graph = durable(&client, &store).graph().unwrap();
    store.fail(op, 1);
    let failure =
        failure(call_durable(&Host::new(), &graph, opening(&[said("i1", "hi")]), "", "r1").await);
    let (node, error) = failed_node(&failure.error);
    (node.to_owned(), store_error(error))
}

#[tokio::test]
async fn given_a_store_that_fails_a_write_when_the_agent_runs_then_the_writing_node_fails_with_thread_carrying_its_error()
 {
    for (op, node) in [
        (Op::Create, "enter"),
        (Op::Read, "enter"),
        (Op::StoreInput, "enter"),
        (Op::OpenTurn, "llm"),
        (Op::PushEvent, "llm"),
        (Op::PushResults, "tools"),
        (Op::EndTurn, "llm"),
    ] {
        assert_eq!(
            failing_on(op).await,
            (node.to_owned(), StoreError::Injected(op)),
            "{op:?}"
        );
    }
}

#[tokio::test]
async fn given_a_thread_held_for_another_agent_when_the_agent_asks_for_its_thread_then_thread() {
    let store = Arc::new(TestStore::new());
    let other = Origin {
        run: crate::origin::RunId::new("another run"),
        occurrence: Default::default(),
    };
    store.create(&nth_thread(1), &other).await.unwrap();
    let client = mock(vec![text_step("hello")]);
    let graph = durable(&client, &store).graph().unwrap();

    let failure =
        failure(call_durable(&Host::new(), &graph, opening(&[said("i1", "hi")]), "", "r1").await);

    let (node, error) = failed_node(&failure.error);
    assert_eq!(node, "enter");
    assert!(matches!(store_error(error), StoreError::Conflict(_)));
    assert_eq!(client.calls(), 0);
}
