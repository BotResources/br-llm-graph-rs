use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    ToolResultBlock, Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource,
};
use serde_json::{Value as Json, json};

use crate::graph::{Always, Context, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::model::ToolSpec;
use crate::react::test_support::{author, config, key, schema};
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::react::tool_node::ToolNode;
use crate::run::{Outcome, channel, run};
use crate::state::{Config, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

#[derive(Default)]
struct GaugeTool {
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl Tool for GaugeTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: ToolName::new("gauge").unwrap(),
            description: "measures overlap".to_owned(),
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
        Box::pin(async move {
            let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
            self.live.fetch_sub(1, Ordering::SeqCst);
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
    let tool = Arc::new(GaugeTool::default());
    let node = ToolNode {
        key: key("chat"),
        author: author(),
        tools: vec![tool.clone()],
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(2).unwrap())),
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
    assert_eq!(tool.peak.load(Ordering::SeqCst), 2);
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
