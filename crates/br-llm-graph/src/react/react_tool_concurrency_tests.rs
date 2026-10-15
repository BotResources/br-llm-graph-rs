use std::collections::BTreeMap;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context as TaskContext, Poll, Wake, Waker};

use br_llm_clients_core::{OutputMode, ToolSpec};
use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    ToolResultBlock, Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource,
};
use serde_json::{Value as Json, json};

use crate::graph::{Always, Context, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::react_agent::ReactAgent;
use crate::react::source::Source;
use crate::react::test_support::{
    EchoTool, author, calls_step, config, key, mock, schema, spec, text_step,
};
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::react::tool_gate::SharedGates;
use crate::react::tool_node::ToolNode;
use crate::run::{Outcome, channel, run};
use crate::state::{Config, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

/// How many calls of the gauge tools run at once, and the most at once.
#[derive(Default)]
struct Gauge {
    live: AtomicUsize,
    peak: AtomicUsize,
}

/// A tool that measures its overlap with the other calls of its gauge.
struct GaugeTool {
    spec: ToolSpec,
    gauge: Arc<Gauge>,
}

impl GaugeTool {
    fn new(name: &'static str, gauge: &Arc<Gauge>) -> Arc<Self> {
        Arc::new(Self {
            spec: spec(name),
            gauge: gauge.clone(),
        })
    }
}

impl Tool for GaugeTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            let now = self.gauge.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.gauge.peak.fetch_max(now, Ordering::SeqCst);
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
            self.gauge.live.fetch_sub(1, Ordering::SeqCst);
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("measured").unwrap(),
            )]))
        })
    }
}

fn four_calls_pending() -> State {
    let calls = (1..=4)
        .map(|index| {
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new(format!("c{index}")).unwrap(),
                name: ToolName::new("gauge").unwrap(),
                arguments: json!({}),
            })
        })
        .collect();
    let step = Step::new(calls, StopReason::AwaitingToolResults, None, None).unwrap();
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("measure").unwrap())],
        )
        .unwrap(),
    );
    chat.push_turn(Turn::new(TurnId::new("t0").unwrap(), Some(author()), step))
        .unwrap();
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(key("enabled"), Value::list(Vec::new()));
    State::new(schema(), values).unwrap()
}

#[tokio::test]
async fn given_a_tool_node_limit_when_several_calls_are_pending_then_at_most_that_many_run_at_once()
{
    let gauge = Arc::new(Gauge::default());
    let node = ToolNode {
        key: key("chat"),
        author: author(),
        tools: vec![GaugeTool::new("gauge", &gauge)],
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(2).unwrap())),
        enabled: None,
        gates: Arc::new(SharedGates::default()),
    };
    let graph = GraphBuilder::new(schema())
        .entry(NodeId::new("tools").unwrap())
        .node(NodeId::new("tools").unwrap(), node)
        .edge(
            NodeId::new("tools").unwrap(),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut inbox) = channel();

    let outcome = run(
        &graph,
        &config(),
        four_calls_pending(),
        None,
        &ctx,
        &mut inbox,
    )
    .await
    .map_err(|f| f.error)
    .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(gauge.peak.load(Ordering::SeqCst), 2);
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    let Some(TurnItem::ToolResults(results)) = turn.items().last() else {
        panic!("expected tool results");
    };
    let ids: Vec<&str> = results
        .results()
        .iter()
        .map(|result| result.tool_call_id.as_str())
        .collect();
    assert_eq!(ids, vec!["c1", "c2", "c3", "c4"]);
}

/// An agent whose tools `left` and `right` are run by two tool nodes, with a
/// tool concurrency of 2. On each of `runs` runs started together, its model
/// calls each tool three times, then answers.
fn split_agent(gauge: &Arc<Gauge>, runs: usize) -> Arc<crate::graph::Graph> {
    let left = GaugeTool::new("left", gauge);
    let right = GaugeTool::new("right", gauge);
    let calls = calls_step(&["left", "left", "left", "right", "right", "right"]);
    let mut steps = vec![calls; runs];
    steps.extend(vec![text_step("measured"); runs]);
    ReactAgent {
        author: author(),
        client: Arc::new(mock(steps)),
        system: vec![Source::Const("Measure.".to_owned())],
        output_mode: OutputMode::Plain,
        tools: vec![left.clone(), right.clone()],
        tool_nodes: vec![
            (
                NodeId::new("left_tools").unwrap(),
                vec![left as Arc<dyn Tool>],
            ),
            (
                NodeId::new("right_tools").unwrap(),
                vec![right as Arc<dyn Tool>],
            ),
        ],
        tool_concurrency: Some(Limit::Fixed(NonZeroUsize::new(2).unwrap())),
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
    }
    .graph()
    .unwrap()
}

async fn run_split_agent(graph: &crate::graph::Graph) {
    let mut history = Conversation::new();
    history.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("measure").unwrap())],
        )
        .unwrap(),
    );
    let state = graph
        .start_state([(key("history"), Value::conversation(history))])
        .unwrap();
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut inbox) = channel();
    let outcome = run(graph, &config, state, None, &ctx, &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
}

#[tokio::test]
async fn given_an_agent_whose_tools_are_split_between_nodes_when_calls_are_pending_then_the_limit_bounds_them_all_together()
 {
    let gauge = Arc::new(Gauge::default());
    run_split_agent(&split_agent(&gauge, 1)).await;
    assert_eq!(gauge.peak.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn given_two_runs_of_one_agent_graph_at_once_when_calls_are_pending_then_each_run_has_its_own_limit()
 {
    let gauge = Arc::new(Gauge::default());
    let graph = split_agent(&gauge, 2);
    futures_util::join!(run_split_agent(&graph), run_split_agent(&graph));
    assert_eq!(gauge.peak.load(Ordering::SeqCst), 4);
}

/// A future pending `n` times, waking its task each time, as a tool
/// streaming I/O does.
struct Busy(usize);

impl Future for Busy {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        if self.0 == 0 {
            return Poll::Ready(());
        }
        self.0 -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// A tool busy for `{"yields": n}` polls of its task.
struct BusyTool(ToolSpec);

impl Tool for BusyTool {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn call<'a>(
        &'a self,
        arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            Busy(usize::try_from(arguments["yields"].as_u64().unwrap_or(0)).unwrap_or(0)).await;
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("busy done").unwrap(),
            )]))
        })
    }
}

/// The waker of the task that runs the node: its strong count tells the live
/// clones, and it counts its wakes.
#[derive(Default)]
struct Counting {
    wakes: AtomicUsize,
}

impl Wake for Counting {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn given_calls_waiting_for_a_busy_slot_when_their_task_is_polled_again_and_again_then_the_gate_keeps_one_waker_per_waiting_call()
 {
    // R3b's probe: one call busy for 2000 polls holds the only slot while
    // twenty calls wait for it.
    use crate::react::react_hook_support::{answered, call, pending, run_tools, tool_graph};
    let mut calls = vec![call("c0", "busy", json!({ "yields": 2000 }))];
    calls.extend((1..=20).map(|index| call(&format!("c{index}"), "echo", json!({}))));
    let busy: Arc<dyn Tool> = Arc::new(BusyTool(spec("busy")));
    let graph = tool_graph(vec![busy, Arc::new(EchoTool)], Some(1), false);
    let counter = Arc::new(Counting::default());
    let waker = Waker::from(counter.clone());
    let mut cx = TaskContext::from_waker(&waker);
    let mut run = Box::pin(run_tools(&graph, pending(calls)));

    let mut peak = 0;
    let outcome = loop {
        if let Poll::Ready(outcome) = run.as_mut().poll(&mut cx) {
            break outcome;
        }
        peak = peak.max(Arc::strong_count(&counter));
    };

    assert_eq!(answered(outcome).len(), 21);
    assert!(
        peak <= 25,
        "live waker clones peaked at {peak} for twenty waiting calls: the counter, the task's \
         waker, one per waiting call and a margin of three"
    );
}
