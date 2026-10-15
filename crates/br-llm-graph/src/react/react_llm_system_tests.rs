//! The system text of the model-call step: the texts of its sources, in
//! order, joined by a blank line, and how each kind of source resolves.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::GraphError;
use crate::react::react_llm_tests::{ctx, llm, run_on, single};
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::state::{Config, Kind, Schema, State, Value};

#[tokio::test]
async fn given_multiple_system_sources_when_llm_runs_then_joined_by_blank_line() {
    let client = mock(vec![text_step("done")]);
    let node = llm(
        &client,
        vec![Source::Config(key("base")), Source::State(key("enabled"))],
    );

    run_on(&single(node), seeded_state(), &ctx()).await.unwrap();

    assert_eq!(
        system_of(&requests(&client)[0]).unwrap(),
        "You are a helpful agent.\n\necho\n\ntally"
    );
}

#[tokio::test]
async fn given_a_constant_and_a_computed_source_when_llm_runs_then_both_are_in_the_system_text() {
    let client = mock(vec![text_step("done")]);
    let computed = Source::Fn(Arc::new(|state, config| {
        let tools = state.list(&key("enabled"))?.len();
        Ok(format!("{} Tools: {tools}.", config.str(&key("base"))?))
    }));
    let node = llm(
        &client,
        vec![Source::Const("Be brief.".to_owned()), computed],
    );

    run_on(&single(node), seeded_state(), &ctx()).await.unwrap();

    assert_eq!(
        system_of(&requests(&client)[0]).unwrap(),
        "Be brief.\n\nYou are a helpful agent. Tools: 2."
    );
}

#[tokio::test]
async fn given_no_system_source_or_only_blank_texts_when_llm_runs_then_no_system_text() {
    for system in [
        Vec::new(),
        vec![Source::Const(String::new()), Source::Const("  ".to_owned())],
    ] {
        let client = mock(vec![text_step("done")]);

        run_on(&single(llm(&client, system)), seeded_state(), &ctx())
            .await
            .unwrap();

        assert_eq!(system_of(&requests(&client)[0]), None);
    }
}

#[test]
fn given_each_kind_of_source_when_resolved_then_one_text() {
    let (state, config) = (seeded_state(), config());
    let resolved = |source: Source| source.resolve(&state, &config).unwrap();
    assert_eq!(
        resolved(Source::Config(key("base"))),
        "You are a helpful agent."
    );
    assert_eq!(resolved(Source::State(key("enabled"))), "echo\n\ntally");
    assert_eq!(resolved(Source::Const("fixed".to_owned())), "fixed");
    assert_eq!(
        resolved(Source::Fn(Arc::new(|state, _config| {
            Ok(state.list(&key("log"))?.len().to_string())
        }))),
        "0"
    );
    let failing = Source::Fn(Arc::new(|state, _config| {
        Ok(state.str(&key("missing"))?.to_owned())
    }));
    assert!(matches!(
        failing.resolve(&state, &config),
        Err(GraphError::UnknownKey { .. })
    ));
}

#[test]
fn given_a_source_key_holding_another_kind_when_resolved_then_kind_mismatch() {
    let schema = Schema::builder()
        .state(key("count"), Kind::Int)
        .state(key("counts"), Kind::list(Kind::Int))
        .build();
    let state = State::new(
        schema.clone(),
        BTreeMap::from([
            (key("count"), Value::int(3)),
            (key("counts"), Value::list(vec![Value::int(1)])),
        ]),
    )
    .unwrap();
    let config = Config::new(&schema, BTreeMap::new()).unwrap();
    for name in ["count", "counts"] {
        assert!(
            matches!(
                Source::State(key(name)).resolve(&state, &config),
                Err(GraphError::KindMismatch { key: found, expected: Kind::Str, found: "int" })
                    if found == key(name)
            ),
            "{name}: a string, or a list holding only strings"
        );
    }
}
