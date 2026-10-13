use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::{Always, Context, Graph, GraphBuilder, Limit, Switch, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::OutputMode;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::run::{channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::{SeqIds, refusal};
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn thinking_schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .config(key("base"), Kind::Str)
        .config(key("deep"), Kind::Bool)
        .build()
}

fn thinking_config(deep: bool) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    values.insert(key("deep"), Value::bool(deep));
    Config::new(&thinking_schema(), values).unwrap()
}

fn llm(model: Arc<LoggingModel>, thinking: Option<Switch>) -> LlmNode {
    LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Text,
        thinking,
    }
}

fn single(node: LlmNode) -> Result<Graph, GraphError> {
    GraphBuilder::new(thinking_schema())
        .entry(nid("llm"))
        .node(nid("llm"), node)
        .edge(
            nid("llm"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
}

async fn thinking_sent(thinking: Option<Switch>, deep: bool) -> Option<bool> {
    let model = Arc::new(LoggingModel::new(vec![text_step("ok")]));
    let graph = single(llm(model.clone(), thinking)).unwrap();
    let chat = seeded_state().get(&key("chat")).unwrap().clone();
    let state = State::new(thinking_schema(), BTreeMap::from([(key("chat"), chat)])).unwrap();
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut inbox) = channel();
    run(
        &graph,
        &thinking_config(deep),
        state,
        None,
        &ctx,
        &mut inbox,
    )
    .await
    .map_err(|f| f.error)
    .unwrap();
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    requests[0].thinking
}

#[tokio::test]
async fn given_no_thinking_switch_when_llm_runs_then_the_request_leaves_the_provider_default() {
    assert_eq!(thinking_sent(None, true).await, None);
}

#[tokio::test]
async fn given_fixed_thinking_switch_when_llm_runs_then_the_request_carries_it() {
    assert_eq!(
        thinking_sent(Some(Switch::Fixed(true)), false).await,
        Some(true)
    );
    assert_eq!(
        thinking_sent(Some(Switch::Fixed(false)), true).await,
        Some(false)
    );
}

#[tokio::test]
async fn given_config_thinking_switch_when_llm_runs_then_each_run_reads_its_config() {
    let switch = Switch::Config(key("deep"));
    assert_eq!(thinking_sent(Some(switch.clone()), true).await, Some(true));
    assert_eq!(thinking_sent(Some(switch), false).await, Some(false));
}

#[test]
fn given_thinking_switch_on_a_non_bool_config_key_when_built_then_refused() {
    let model = Arc::new(LoggingModel::new(Vec::new()));
    for name in ["base", "missing"] {
        assert!(matches!(
            refusal(single(llm(model.clone(), Some(Switch::Config(key(name)))))),
            GraphError::SwitchKeyMismatch { key: found } if found == key(name)
        ));
    }
}

#[test]
fn given_thinking_switch_on_a_non_bool_key_in_a_limited_loop_when_built_then_refused() {
    let model = Arc::new(LoggingModel::new(Vec::new()));
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: Vec::new(),
        after: Target::End(EndLabel::new("done").unwrap()),
        tool_concurrency: None,
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(2).unwrap()),
            on_limit: OnLimit::Error,
        }),
    };
    let built = react
        .add(
            GraphBuilder::new(thinking_schema()).entry(nid("llm")),
            llm(model, Some(Switch::Config(key("base")))),
        )
        .unwrap()
        .build();
    assert!(matches!(
        refusal(built),
        GraphError::SwitchKeyMismatch { .. }
    ));
}
