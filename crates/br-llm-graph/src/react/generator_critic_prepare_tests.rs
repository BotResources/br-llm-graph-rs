use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_clients_mock::MockClient;
use br_llm_messages::{Conversation, InputId, Text, UserBlock, UserInput, UserSource};

use crate::error::{GraphError, NodeFault};
use crate::graph::{
    Always, CaptureSource, CaptureUpdate, CheckSite, Context, Count, GraphBuilder, Input, Node,
    NodeFuture, OnFailure, Output, SubGraph, Target,
};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::test_support::*;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

/// A document source standing for a service: it counts its reads.
struct Documents {
    texts: BTreeMap<String, String>,
    reads: AtomicUsize,
}

impl Documents {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            texts: BTreeMap::from([("doc-7".to_owned(), "Tides follow the moon.".to_owned())]),
            reads: AtomicUsize::new(0),
        })
    }

    fn read(&self, id: &str) -> Result<String, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.texts
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no document {id}"))
    }
}

/// Opens the conversation with the document `document_id` names.
struct OpenDocument {
    documents: Arc<Documents>,
}

impl Node for OpenDocument {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let text = self.documents.read(state.str(&key("document_id"))?)?;
            let input = UserInput::new(
                InputId::new(format!("i-{text}"))?,
                UserSource::Human,
                None,
                vec![UserBlock::text(Text::new(format!("Summarize: {text}"))?)],
            )?;
            Ok(vec![Update::Input {
                key: key("opening"),
                input,
            }])
        })
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        match schema.state.get(&key("document_id")) {
            Some(Kind::Str) => Ok(()),
            _ => Err(GraphError::UnknownKey {
                key: key("document_id"),
            }),
        }
    }
}

fn prepared(
    documents: Arc<Documents>,
    generator_client: &MockClient,
    critic_client: &MockClient,
) -> GeneratorCritic {
    GeneratorCritic {
        inputs: vec![(key("document_id"), Kind::Str)],
        prepare: Some(Box::new(OpenDocument { documents })),
        ..GeneratorCritic::new(
            generator(generator_client),
            critic(critic_client),
            Count::Fixed(1),
        )
    }
}

async fn open(graph: &crate::graph::Graph, document_id: &str) -> Ran {
    call_with(
        graph,
        Conversation::new(),
        vec![(key("document_id"), Value::str(document_id))],
        BTreeMap::new(),
    )
    .await
}

#[tokio::test]
async fn given_a_prepare_node_and_an_empty_conversation_when_run_then_it_opens_the_conversation_once()
 {
    let documents = Documents::new();
    let generator_client = mock(vec![text_step("The moon pulls.")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = prepared(documents.clone(), &generator_client, &critic_client)
        .graph()
        .unwrap();
    assert_eq!(graph.entry().as_str(), "prepare");
    let state = ended(open(&graph, "doc-7").await, "validated");
    assert_eq!(state.str(&key("answer")).unwrap(), "The moon pulls.");
    assert_eq!(
        shape(state.transcript()),
        ["user", "writer:The moon pulls.", "reviewer:Accepted."]
    );
    let opening =
        user_message("<message role=\"user\">Summarize: Tides follow the moon.</message>");
    assert_eq!(
        requests(&generator_client)[0].messages(),
        std::slice::from_ref(&opening)
    );
    assert_eq!(requests(&critic_client)[0].messages()[0], opening);
    assert_eq!(documents.reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn given_a_prepare_node_that_fails_when_run_then_the_generator_critic_fails_before_any_model_call()
 {
    let generator_client = mock(Vec::new());
    let critic_client = mock(Vec::new());
    let graph = prepared(Documents::new(), &generator_client, &critic_client)
        .graph()
        .unwrap();
    let failure = open(&graph, "doc-404").await.result.err().unwrap();
    match failure.error {
        GraphError::NodeFailed {
            node,
            source: NodeFault::Returned(error),
        } => {
            assert_eq!(node.as_str(), "prepare");
            assert_eq!(error.to_string(), "no document doc-404");
        }
        other => panic!("expected the prepare node to fail, got: {other}"),
    }
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_a_failing_prepare_called_with_a_capture_when_run_then_the_caller_goes_on_with_the_reason()
 {
    let graph = prepared(Documents::new(), &mock(Vec::new()), &mock(Vec::new()))
        .graph()
        .unwrap();
    let call = SubGraph::call(graph)
        .input(
            key("opening"),
            Input::Const(Value::conversation(Conversation::new())),
        )
        .input(key("document_id"), Input::From(key("id")))
        .output(key("answer"), Output::Set(key("summary")))
        .on_failure(OnFailure::Capture(vec![CaptureUpdate::Set(
            key("failure"),
            CaptureSource::Reason,
        )]));
    let schema = Schema::builder()
        .state_by_name(key("id"), Kind::Str)
        .state_by_name(key("summary"), Kind::Str)
        .state_by_name(key("failure"), Kind::Str)
        .build();
    let caller = GraphBuilder::new(schema)
        .entry(NodeId::new("summarize").unwrap())
        .subgraph(NodeId::new("summarize").unwrap(), call)
        .edge(
            NodeId::new("summarize").unwrap(),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("id"))
        .build()
        .unwrap();
    let state = caller
        .start_state([(key("id"), Value::str("doc-404"))])
        .unwrap();
    let config = Config::new(caller.schema(), BTreeMap::new()).unwrap();
    let state = ended(run_in(&caller, state, &config, &ctx()).await, "done");
    assert_eq!(state.str(&key("summary")).unwrap(), "");
    assert!(
        state
            .str(&key("failure"))
            .unwrap()
            .contains("no document doc-404"),
        "{}",
        state.str(&key("failure")).unwrap()
    );
}

#[test]
fn given_a_prepare_node_that_refuses_the_schema_when_built_then_invalid_node() {
    let built = GeneratorCritic {
        prepare: Some(Box::new(OpenDocument {
            documents: Documents::new(),
        })),
        ..GeneratorCritic::new(
            generator(&mock(Vec::new())),
            critic(&mock(Vec::new())),
            Count::Fixed(1),
        )
    }
    .graph();
    assert!(matches!(
        built,
        Err(GraphError::InvalidNode { ref node, .. }) if node.as_str() == "prepare"
    ));
}

#[test]
fn given_a_tool_node_named_like_a_node_of_the_generator_critic_when_built_then_duplicate_node() {
    let client = || mock(Vec::new());
    for reserved in [
        "prepare", "route", "generate", "check", "critique", "finish",
    ] {
        let echo: Arc<dyn crate::react::tool::Tool> = Arc::new(EchoTool);
        let built = GeneratorCritic {
            generator: crate::react::seats::GeneratorSeat {
                tools: vec![echo.clone()],
                tool_nodes: vec![(NodeId::new(reserved).unwrap(), vec![echo])],
                ..generator(&client())
            },
            ..prepared(Documents::new(), &client(), &client())
        }
        .graph();
        assert!(
            matches!(built, Err(GraphError::DuplicateNode { ref id }) if id.as_str() == reserved),
            "{reserved}"
        );
    }
}
