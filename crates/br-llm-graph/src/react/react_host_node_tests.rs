//! A node the host writes on the public helpers: `wire` renders a
//! conversation key for its author, `complete` calls a client with the
//! observer as sink, `structured` reads the reply into the host's type.

use std::sync::Arc;

use br_llm_clients_core::{
    ClientError, ClientRequest, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_clients_mock::MockClient;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Always, Context, FnNode, Graph, GraphBuilder, NodeFuture, Target};
use crate::observe::Observer;
use crate::react::helpers::{structured, wire};
use crate::react::sink::complete;
use crate::react::test_support::*;
use crate::run::{Outcome, control, run};
use crate::state::{Config, State, Value};
use crate::testkit::SeqIds;
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

#[derive(serde::Deserialize)]
struct Verdict {
    validated: bool,
}

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn shape() -> StructuredShape {
    StructuredShape::new(
        "verdict",
        None,
        Parameters::new(vec![
            Parameter::new("validated", "", true, ParameterKind::Boolean).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap()
}

/// A graph of one host node, `judge`, that sends `chat` rendered for the
/// agent to `client` in structured mode, its events under the key `chat`,
/// and appends `valid` or `invalid` to `log` from the verdict it reads.
fn judge(client: &MockClient) -> Graph {
    let client = Arc::new(client.clone());
    let node = FnNode::new(
        move |state: &State, _config: &Config, ctx: &Context| -> NodeFuture<'_> {
            let client = client.clone();
            Box::pin(async move {
                let messages = wire(state.conversation(&key("chat"))?, &author())?;
                let request = ClientRequest::new(
                    None,
                    messages,
                    Vec::new(),
                    OutputMode::Structured(shape()),
                )?;
                let step = complete(client.as_ref(), &request, ctx, &key("chat")).await?;
                let verdict: Verdict = structured(&step)?;
                let word = if verdict.validated {
                    "valid"
                } else {
                    "invalid"
                };
                Ok(vec![Update::Append {
                    key: key("log"),
                    value: Value::str(word),
                }])
            })
        },
    );
    GraphBuilder::new(schema())
        .entry(nid("judge"))
        .node(nid("judge"), node)
        .edge(
            nid("judge"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

#[tokio::test]
async fn given_a_host_node_on_the_helpers_when_run_then_it_reads_the_reply_and_the_observer_gets_its_events()
 {
    let client = mock(vec![structured_step(json!({ "validated": true }))]);
    let recorder = Arc::new(StreamRecorder::default());
    let observer: Arc<dyn Observer> = recorder.clone();
    let ctx = Context::new(observer, Arc::new(SeqIds::new()));
    let (_sender, mut channel) = control();

    let outcome = run(
        &judge(&client),
        &config(),
        seeded_state(),
        None,
        &ctx,
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();

    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("valid")]);
    let sent = requests(&client);
    assert_eq!(
        sent[0].messages(),
        wire(
            seeded_state().conversation(&key("chat")).unwrap(),
            &author()
        )
        .unwrap()
        .as_slice()
    );
    let events = recorder.events();
    assert!(!events.is_empty());
    assert!(
        events
            .iter()
            .all(|(occurrence, key, _)| occurrence == "judge" && key == "chat")
    );
}

#[tokio::test]
async fn given_a_failing_client_when_a_host_node_completes_then_model_carrying_the_client_error() {
    let client = failing(ClientError::Unavailable { retry_after: None });
    let (_sender, mut channel) = control();

    let failure = run(
        &judge(&client),
        &config(),
        seeded_state(),
        None,
        &ctx(),
        &mut channel,
    )
    .await
    .err()
    .unwrap();

    assert!(matches!(
        returned(&failure.error),
        GraphError::Model(ClientError::Unavailable { retry_after: None })
    ));
}

fn ctx() -> Context {
    Context::new(
        Arc::new(crate::observe::NoopObserver),
        Arc::new(SeqIds::new()),
    )
}
