use std::sync::Arc;

use serde_json::json;

use crate::graph::{Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::OutputMode;
use crate::react::test_support::*;
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
