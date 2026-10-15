//! A call mapped through typed keys: every typed mapping and capture lands
//! where the by-name one would, and the build still refuses a typed key of
//! another schema.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::subgraph::{CaptureUpdate, OnFailure, SubGraph};
use crate::graph::{Always, Context, Graph, GraphBuilder, Node, NodeFuture, Target};
use crate::observe::NoopObserver;
use crate::run::{Outcome, control, run};
use crate::state::{Config, ConfigKey, Schema, State, StateKey, Value};
use crate::value::{EndLabel, NodeId};

/// The called graph's keys.
#[derive(Clone)]
struct Child {
    text: StateKey<String>,
    answer: StateKey<String>,
    rounds_out: StateKey<i64>,
    rounds: ConfigKey<i64>,
    prefix: ConfigKey<String>,
}

/// Writes `answer` as `<prefix><text>#<rounds>` and `rounds_out`; fails on a
/// text starting with `bad`.
impl Node for Child {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, _: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let text = state.get(&self.text)?;
            if text.starts_with("bad") {
                return Err(format!("cannot handle {text}").into());
            }
            let rounds = config.get(&self.rounds)?;
            let prefix = config.get(&self.prefix)?;
            Ok(vec![
                self.answer.set(format!("{prefix}{text}#{rounds}"))?,
                self.rounds_out.set(rounds)?,
            ])
        })
    }
}

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

fn child() -> (Child, Arc<Graph>) {
    let mut schema = Schema::builder();
    let keys = Child {
        text: schema.state("text").unwrap(),
        answer: schema.state("answer").unwrap(),
        rounds_out: schema.state("rounds_out").unwrap(),
        rounds: schema.config("rounds").unwrap(),
        prefix: schema.config("prefix").unwrap(),
    };
    let graph = GraphBuilder::new(schema.build())
        .entry(nid("work"))
        .node(nid("work"), keys.clone())
        .edge(nid("work"), Always(end("approved")))
        .input(&keys.text)
        .output(&keys.answer)
        .output(&keys.rounds_out)
        .build()
        .unwrap();
    (keys, Arc::new(graph))
}

/// The caller's keys.
struct Parent {
    schema: Schema,
    question: StateKey<String>,
    result: StateKey<String>,
    results: StateKey<Vec<String>>,
    steps: StateKey<i64>,
    counts: StateKey<Vec<i64>>,
    label: StateKey<String>,
    labels: StateKey<Vec<String>>,
    tone_state: StateKey<String>,
    rounds: ConfigKey<i64>,
    tone: ConfigKey<String>,
}

fn parent() -> Parent {
    let mut schema = Schema::builder();
    let question = schema.state("question").unwrap();
    let result = schema.state("result").unwrap();
    let results = schema.state("results").unwrap();
    let steps = schema.state("steps").unwrap();
    let counts = schema.state("counts").unwrap();
    let label = schema.state("label").unwrap();
    let labels = schema.state("labels").unwrap();
    let tone_state = schema
        .state_with_default("tone_state", "loud:".to_owned())
        .unwrap();
    let rounds = schema.config("rounds").unwrap();
    let tone = schema.config("tone").unwrap();
    Parent {
        schema: schema.build(),
        question,
        result,
        results,
        steps,
        counts,
        label,
        labels,
        tone_state,
        rounds,
        tone,
    }
}

/// Runs a caller whose single node makes `call`, with `question` given.
async fn called(parent: &Parent, call: SubGraph, question: &str) -> Result<State, GraphError> {
    let graph = GraphBuilder::new(parent.schema.clone())
        .entry(nid("ask"))
        .subgraph(nid("ask"), call)
        .edge(nid("ask"), Always(end("done")))
        .input(&parent.question)
        .build()?;
    let state = graph.start_state([parent.question.given(question)?])?;
    let config = Config::new(
        &parent.schema,
        BTreeMap::from([
            (parent.rounds.key().clone(), Value::int(3)),
            (parent.tone.key().clone(), Value::str("calm:")),
        ]),
    )?;
    let ctx = crate::testkit::context(Arc::new(NoopObserver));
    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &ctx, &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => Ok(state),
        Ok(_) => panic!("the run did not finish"),
        Err(failure) => Err(failure.error),
    }
}

#[tokio::test]
async fn given_typed_mappings_from_the_callers_keys_when_called_then_the_outputs_land_typed() {
    let (keys, graph) = child();
    let p = parent();
    let call = SubGraph::call(graph)
        .input_from(&keys.text, &p.question)
        .config_from(&keys.rounds, &p.rounds)
        .config_from(&keys.prefix, &p.tone)
        .output_set(&keys.answer, &p.result)
        .output_append(&keys.answer, &p.results)
        .output_set(&keys.rounds_out, &p.steps)
        .output_append(&keys.rounds_out, &p.counts)
        .end_label_set(&p.label);
    let state = called(&p, call, "q").await.unwrap();
    assert_eq!(state.get(&p.result).unwrap(), "calm:q#3");
    assert_eq!(state.get(&p.results).unwrap(), vec!["calm:q#3"]);
    assert_eq!(state.get(&p.steps).unwrap(), 3);
    assert_eq!(state.get(&p.counts).unwrap(), vec![3]);
    assert_eq!(state.get(&p.label).unwrap(), "approved");
}

#[tokio::test]
async fn given_typed_mappings_from_values_and_across_state_and_configuration_when_called_then_read()
{
    let (keys, graph) = child();
    let p = parent();
    let call = SubGraph::call(graph)
        .input_value(&keys.text, "fixed")
        .unwrap()
        .config_value(&keys.rounds, 7)
        .unwrap()
        .config_from_state(&keys.prefix, &p.tone_state)
        .output_set(&keys.answer, &p.result)
        .end_label_append(&p.labels);
    let state = called(&p, call, "ignored").await.unwrap();
    assert_eq!(state.get(&p.result).unwrap(), "loud:fixed#7");
    assert_eq!(state.get(&p.labels).unwrap(), vec!["approved"]);
}

#[tokio::test]
async fn given_a_child_input_from_a_caller_configuration_key_when_called_then_read() {
    let (keys, graph) = child();
    let p = parent();
    let call = SubGraph::call(graph)
        .input_from_config(&keys.text, &p.tone)
        .config_value(&keys.rounds, 1)
        .unwrap()
        .config_value(&keys.prefix, "")
        .unwrap()
        .output_set(&keys.answer, &p.result);
    let state = called(&p, call, "q").await.unwrap();
    assert_eq!(state.get(&p.result).unwrap(), "calm:#1");
}

#[tokio::test]
async fn given_typed_captures_when_the_child_fails_then_each_lands_typed() {
    let (keys, graph) = child();
    let p = parent();
    let call = SubGraph::call(graph)
        .input_from(&keys.text, &p.question)
        .config_from(&keys.rounds, &p.rounds)
        .config_from(&keys.prefix, &p.tone)
        .output_set(&keys.answer, &p.result)
        .on_failure(OnFailure::Capture(vec![
            CaptureUpdate::set_reason(&p.result),
            CaptureUpdate::set_value(&p.steps, -1).unwrap(),
            CaptureUpdate::set_from(&p.label, &p.question),
            CaptureUpdate::append_reason(&p.results),
            CaptureUpdate::append_value(&p.counts, 0).unwrap(),
            CaptureUpdate::append_from(&p.labels, &p.tone_state),
        ]));
    let state = called(&p, call, "bad input").await.unwrap();
    let reason = state.get(&p.result).unwrap();
    assert!(reason.contains("cannot handle bad input"), "{reason}");
    assert_eq!(state.get(&p.steps).unwrap(), -1);
    assert_eq!(state.get(&p.label).unwrap(), "bad input");
    assert_eq!(state.get(&p.results).unwrap(), vec![reason]);
    assert_eq!(state.get(&p.counts).unwrap(), vec![0]);
    assert_eq!(state.get(&p.labels).unwrap(), vec!["loud:"]);
}

#[test]
fn given_a_float_that_is_not_finite_when_mapped_or_captured_then_float_not_finite() {
    let (_, graph) = child();
    let mut other = Schema::builder();
    let ratio: StateKey<f64> = other.state("ratio").unwrap();
    let ratios: StateKey<Vec<f64>> = other.state("ratios").unwrap();
    let scale: ConfigKey<f64> = other.config("scale").unwrap();
    assert!(matches!(
        SubGraph::call(graph.clone()).input_value(&ratio, f64::NAN),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        SubGraph::call(graph).config_value(&scale, f64::INFINITY),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        CaptureUpdate::set_value(&ratio, f64::NAN),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        CaptureUpdate::append_value(&ratios, f64::NEG_INFINITY),
        Err(GraphError::FloatNotFinite)
    ));
}

#[tokio::test]
async fn given_a_typed_key_of_another_schema_when_mapped_then_the_build_still_refuses_it() {
    let (keys, graph) = child();
    let p = parent();
    let mut other = Schema::builder();
    let stray: StateKey<String> = other.state("stray").unwrap();
    let call = SubGraph::call(graph)
        .input_from(&keys.text, &stray)
        .config_from(&keys.rounds, &p.rounds)
        .config_from(&keys.prefix, &p.tone);
    match called(&p, call, "q").await {
        Err(GraphError::InvalidNode { source, .. }) => assert!(matches!(
            *source,
            GraphError::SubGraphSourceMismatch { ref key } if key.as_str() == "text"
        )),
        other => panic!("expected InvalidNode, got {other:?}"),
    }
}
