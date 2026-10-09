use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_messages::{
    AssistantBlock, Author, Conversation, Entry, Text, ToolCallId, ToolName, ToolResult,
    ToolResultBlock, Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource, WireMessage,
    WireUserBlock,
};
use serde_json::{Value as Json, json};

use crate::error::{GraphError, NodeFault};
use crate::graph::{
    Always, Context, Graph, GraphBuilder, Input, ItemFailure, Limit, Map, Output, SubGraph, Switch,
    Target,
};
use crate::observe::NoopObserver;
use crate::react::generator_critic::{CriticSeat, GeneratorCritic, GeneratorSeat, verdict_schema};
use crate::react::model::OutputMode;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn writer() -> Author {
    Author::new("writer").unwrap()
}

fn reviewer() -> Author {
    Author::new("reviewer").unwrap()
}

fn limit(value: usize) -> Limit {
    Limit::Fixed(NonZeroUsize::new(value).unwrap())
}

fn question(body: &str) -> Conversation {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new(body).unwrap())],
        )
        .unwrap(),
    );
    chat
}

fn verdict(is_valid: bool, message: &str) -> br_llm_messages::Step {
    structured_step(json!({ "thinking": "weighing it", "is_valid": is_valid, "message": message }))
}

fn generator(model: Arc<LoggingModel>) -> GeneratorSeat {
    GeneratorSeat {
        author: writer(),
        model,
        output: OutputMode::Text,
        thinking: None,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
    }
}

fn critic(model: Arc<LoggingModel>) -> CriticSeat {
    CriticSeat {
        author: reviewer(),
        model,
        thinking: None,
    }
}

fn duo(generator_model: Arc<LoggingModel>, critic_model: Arc<LoggingModel>) -> GeneratorCritic {
    GeneratorCritic::new(generator(generator_model), critic(critic_model), limit(3))
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

async fn call_with(
    graph: &Graph,
    conversation: Conversation,
    generator_history: Conversation,
    config: BTreeMap<crate::value::Key, Value>,
) -> Result<Outcome, RunFailure> {
    let state = graph
        .start_state([
            (key("conversation"), Value::conversation(conversation)),
            (
                key("generator_history"),
                Value::conversation(generator_history),
            ),
            (key("generator_system"), Value::str("Answer the question.")),
            (key("critic_system"), Value::str("Judge the answer.")),
        ])
        .unwrap();
    let config = Config::new(graph.schema(), config).unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx(), &mut inbox).await
}

async fn call(graph: &Graph, conversation: Conversation) -> Result<Outcome, RunFailure> {
    call_with(graph, conversation, Conversation::new(), BTreeMap::new()).await
}

fn ended(result: Result<Outcome, RunFailure>, label: &str) -> State {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, end } => {
            assert_eq!(end.as_str(), label);
            state
        }
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

/// Each entry as `user`, or `author:text` with the text of the turn's final
/// step, or `author:[n items]` for a turn of several items.
fn shape(conversation: &Conversation) -> Vec<String> {
    conversation
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::UserInput(_) => "user".to_owned(),
            Entry::Turn(turn) if turn.items().len() > 1 => {
                format!("{}:[{} items]", turn.author().unwrap(), turn.items().len())
            }
            Entry::Turn(turn) => format!("{}:{}", turn.author().unwrap(), turn_text(turn)),
        })
        .collect()
}

fn turn_text(turn: &Turn) -> String {
    match turn.items().last() {
        Some(TurnItem::Step(step)) => step.text().map(Text::as_str).collect::<Vec<_>>().join(" "),
        Some(TurnItem::ToolResults(_)) | None => String::new(),
    }
}

fn shape_of(state: &State, name: &str) -> Vec<String> {
    shape(state.conversation(&key(name)).unwrap())
}

fn returned_error(result: Result<Outcome, RunFailure>) -> (State, GraphError) {
    let failure = result.err().unwrap();
    match failure.error {
        GraphError::NodeFailed {
            node,
            source: NodeFault::Returned(error),
        } => {
            assert_eq!(node, nid("critique"));
            (
                failure.checkpoint.state,
                *error.downcast::<GraphError>().unwrap(),
            )
        }
        other => panic!("expected the critique node to fail, got: {other}"),
    }
}

#[tokio::test]
async fn given_a_valid_first_answer_when_run_then_validated_and_nothing_is_appended() {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let graph = duo(generator_model, critic_model).graph().unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert!(state.bool(&key("validated")).unwrap());
    assert_eq!(state.int(&key("generations")).unwrap(), 1);
    assert_eq!(state.str(&key("last_critique")).unwrap(), "");
    assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
    assert_eq!(
        shape_of(&state, "generator_history"),
        ["user", "writer:draft"]
    );
    assert_eq!(shape_of(&state, "answer"), ["writer:draft"]);
}

#[tokio::test]
async fn given_a_rejection_then_a_validation_when_run_then_each_seat_reads_its_own_perspective() {
    let generator_model = Arc::new(LoggingModel::new(vec![
        text_step("first"),
        text_step("second"),
    ]));
    let critic_model = Arc::new(LoggingModel::new(vec![
        verdict(false, "add an example"),
        verdict(true, ""),
    ]));
    let graph = duo(generator_model.clone(), critic_model.clone())
        .graph()
        .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(state.int(&key("generations")).unwrap(), 2);
    let expected = [
        "user",
        "writer:first",
        "reviewer:add an example",
        "writer:second",
    ];
    assert_eq!(shape_of(&state, "conversation"), expected);
    assert_eq!(shape_of(&state, "generator_history"), expected);
    assert_eq!(shape_of(&state, "answer"), ["writer:second"]);

    let generator_requests = generator_model.requests();
    assert_eq!(
        generator_requests[0].system.as_deref(),
        Some("Answer the question.")
    );
    let messages = &generator_requests[1].messages;
    assert_eq!(messages.len(), 3);
    assert!(matches!(messages[1], WireMessage::Assistant { .. }));
    let WireMessage::User { content } = &messages[2] else {
        panic!("expected the critique as a framed message");
    };
    assert!(matches!(
        &content[..],
        [WireUserBlock::Text { text }]
            if text.as_str() == "<message author=\"reviewer\" role=\"agent\">add an example</message>"
    ));

    let critic_requests = critic_model.requests();
    assert_eq!(
        critic_requests[0].system.as_deref(),
        Some("Judge the answer.")
    );
    let messages = &critic_requests[1].messages;
    assert_eq!(messages.len(), 3);
    assert!(matches!(
        &messages[1],
        WireMessage::Assistant { content }
            if content == &vec![AssistantBlock::Text { text: Text::new("add an example").unwrap() }]
    ));
    assert!(matches!(messages[2], WireMessage::User { .. }));
}

#[tokio::test]
async fn given_every_answer_rejected_with_a_final_generation_when_run_then_exhausted_on_the_unassessed_answer()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("a"), text_step("b")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(false, "fix it")]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        critic(critic_model.clone()),
        limit(1),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert!(!state.bool(&key("validated")).unwrap());
    assert_eq!(state.int(&key("generations")).unwrap(), 2);
    assert_eq!(state.str(&key("last_critique")).unwrap(), "fix it");
    assert_eq!(shape_of(&state, "answer"), ["writer:b"]);
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user", "writer:a", "reviewer:fix it", "writer:b"]
    );
    assert_eq!(critic_model.requests().len(), 1);
}

#[tokio::test]
async fn given_every_answer_rejected_without_a_final_generation_when_run_then_exhausted_on_the_rejected_answer()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("a"), text_step("b")]));
    let critic_model = Arc::new(LoggingModel::new(vec![
        verdict(false, "fix it"),
        verdict(false, "still wrong"),
    ]));
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(generator_model.clone()),
            critic(critic_model),
            limit(2),
        )
    }
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert!(!state.bool(&key("validated")).unwrap());
    assert_eq!(state.int(&key("generations")).unwrap(), 2);
    assert_eq!(state.str(&key("last_critique")).unwrap(), "still wrong");
    assert_eq!(shape_of(&state, "answer"), ["writer:b"]);
    assert_eq!(
        shape_of(&state, "conversation"),
        [
            "user",
            "writer:a",
            "reviewer:fix it",
            "writer:b",
            "reviewer:still wrong"
        ]
    );
    assert_eq!(generator_model.requests().len(), 2);
}

fn property_names(schema: &Json) -> Vec<String> {
    schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[test]
fn given_the_critic_thinking_or_not_when_the_verdict_schema_is_chosen_then_thinking_comes_first_only_when_emulated()
 {
    let emulated = verdict_schema(false);
    assert_eq!(
        property_names(&emulated),
        ["thinking", "is_valid", "message"]
    );
    assert_eq!(
        emulated["required"],
        json!(["thinking", "is_valid", "message"])
    );
    let native = verdict_schema(true);
    assert_eq!(property_names(&native), ["is_valid", "message"]);
    assert_eq!(native["required"], json!(["is_valid", "message"]));
}

async fn critic_request(
    thinking: Option<Switch>,
    config: BTreeMap<crate::value::Key, Value>,
) -> crate::react::model::Request {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![structured_step(
        json!({ "is_valid": true, "message": "" }),
    )]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat {
            thinking,
            ..critic(critic_model.clone())
        },
        limit(1),
    )
    .graph()
    .unwrap();
    ended(
        call_with(&graph, question("explain"), Conversation::new(), config).await,
        "validated",
    );
    critic_model.requests().remove(0)
}

fn structured_schema(request: &crate::react::model::Request) -> &Json {
    match &request.output {
        OutputMode::Structured { schema } => schema,
        OutputMode::Text => panic!("expected a structured request"),
    }
}

#[tokio::test]
async fn given_a_critic_with_or_without_native_thinking_when_it_judges_then_the_request_carries_the_matching_schema()
 {
    let native = critic_request(Some(Switch::Fixed(true)), BTreeMap::new()).await;
    assert_eq!(native.thinking, Some(true));
    assert_eq!(structured_schema(&native), &verdict_schema(true));

    let emulated = critic_request(Some(Switch::Fixed(false)), BTreeMap::new()).await;
    assert_eq!(emulated.thinking, Some(false));
    assert_eq!(structured_schema(&emulated), &verdict_schema(false));

    let default = critic_request(None, BTreeMap::new()).await;
    assert_eq!(default.thinking, None);
    assert_eq!(structured_schema(&default), &verdict_schema(false));

    for deep in [true, false] {
        let request = critic_request(
            Some(Switch::Config(key("critic_thinks"))),
            BTreeMap::from([(key("critic_thinks"), Value::bool(deep))]),
        )
        .await;
        assert_eq!(request.thinking, Some(deep));
        assert_eq!(structured_schema(&request), &verdict_schema(deep));
        assert!(request.tools.is_empty());
    }
}

#[test]
fn given_a_critic_switch_on_a_key_of_another_kind_when_built_then_refused() {
    let built = GeneratorCritic::new(
        generator(Arc::new(LoggingModel::new(Vec::new()))),
        CriticSeat {
            thinking: Some(Switch::Config(key("rounds"))),
            ..critic(Arc::new(LoggingModel::new(Vec::new())))
        },
        Limit::Config(key("rounds")),
    )
    .graph();
    assert!(matches!(
        crate::testkit::refusal(built),
        GraphError::SwitchKeyMismatch { .. }
    ));
}

#[test]
fn given_both_seats_with_one_author_when_built_then_same_author() {
    let built = GeneratorCritic::new(
        generator(Arc::new(LoggingModel::new(Vec::new()))),
        CriticSeat {
            author: writer(),
            ..critic(Arc::new(LoggingModel::new(Vec::new())))
        },
        limit(1),
    )
    .graph();
    assert!(matches!(built, Err(GraphError::SameAuthor { .. })));
}

#[tokio::test]
async fn given_a_malformed_verdict_when_the_critic_judges_then_a_typed_error_and_no_critique() {
    for malformed in [
        text_step("looks fine to me"),
        structured_step(json!({ "thinking": "hm", "is_valid": "yes", "message": "x" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false, "message": "" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false })),
    ] {
        let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
        let critic_model = Arc::new(LoggingModel::new(vec![malformed]));
        let graph = duo(generator_model, critic_model).graph().unwrap();
        let (state, error) = returned_error(call(&graph, question("explain")).await);
        assert!(matches!(error, GraphError::Structured { .. }), "{error}");
        assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
        assert_eq!(
            shape_of(&state, "generator_history"),
            ["user", "writer:draft"]
        );
    }
}

#[tokio::test]
async fn given_a_generator_with_a_tool_when_run_then_tool_traffic_stays_in_its_own_history() {
    let generator_model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("answer one"),
        tool_call_step("c2", "echo", json!({})),
        text_step("answer two"),
    ]));
    let critic_model = Arc::new(LoggingModel::new(vec![
        verdict(false, "go further"),
        verdict(true, ""),
    ]));
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool)],
            round_limit: Some(RoundLimit {
                max_rounds: limit(1),
                on_limit: OnLimit::Error,
            }),
            ..generator(generator_model)
        },
        critic(critic_model),
        limit(3),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(
        shape_of(&state, "generator_history"),
        [
            "user",
            "writer:[3 items]",
            "reviewer:go further",
            "writer:[3 items]"
        ]
    );
    assert_eq!(
        shape_of(&state, "conversation"),
        [
            "user",
            "writer:answer one",
            "reviewer:go further",
            "writer:answer two"
        ]
    );
    assert_eq!(shape_of(&state, "answer"), ["writer:answer two"]);
}

#[tokio::test]
async fn given_both_histories_ending_on_an_answer_when_called_again_then_the_critic_judges_it_first()
 {
    let mut conversation = question("explain");
    conversation
        .push_turn(Turn::new(
            TurnId::new("g1").unwrap(),
            Some(writer()),
            text_step("saved answer"),
        ))
        .unwrap();
    let mut generator_history = question("explain");
    let mut turn = Turn::new(
        TurnId::new("g1").unwrap(),
        Some(writer()),
        tool_call_step("c1", "echo", json!({})),
    );
    turn.push_result(ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    ))
    .unwrap();
    turn.push_step(text_step("saved answer")).unwrap();
    generator_history.push_turn(turn).unwrap();

    let generator_model = Arc::new(LoggingModel::new(Vec::new()));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let graph = duo(generator_model.clone(), critic_model).graph().unwrap();
    let state = ended(
        call_with(&graph, conversation, generator_history, BTreeMap::new()).await,
        "validated",
    );
    assert_eq!(state.int(&key("generations")).unwrap(), 1);
    assert_eq!(shape_of(&state, "answer"), ["writer:saved answer"]);
    assert_eq!(
        shape_of(&state, "generator_history"),
        ["user", "writer:[3 items]"]
    );
    assert!(generator_model.requests().is_empty());
}

fn caller_schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("best"), Kind::Conversation)
        .state(key("label"), Kind::Str)
        .state(key("questions"), Kind::list(Kind::Conversation))
        .state(key("question"), Kind::Conversation)
        .state(key("answers"), Kind::list(Kind::Conversation))
        .state(key("verdicts"), Kind::list(Kind::Bool))
        .build()
}

fn duo_call(graph: Arc<Graph>, conversation: &str) -> SubGraph {
    SubGraph::call(graph)
        .input(key("conversation"), Input::From(key(conversation)))
        .input(
            key("generator_history"),
            Input::Const(Value::conversation(Conversation::new())),
        )
        .input(
            key("generator_system"),
            Input::Const(Value::str("Answer the question.")),
        )
        .input(
            key("critic_system"),
            Input::Const(Value::str("Judge the answer.")),
        )
}

async fn run_caller(graph: &Graph, state: State) -> State {
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let (_sender, mut inbox) = channel();
    match run(graph, &config, state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap()
    {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

#[tokio::test]
async fn given_the_duo_called_as_a_subgraph_when_run_then_the_caller_reads_the_answer_and_the_label()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("a"), text_step("b")]));
    let critic_model = Arc::new(LoggingModel::new(vec![
        verdict(false, "fix it"),
        verdict(true, ""),
    ]));
    let call = duo_call(duo(generator_model, critic_model).graph().unwrap(), "chat")
        .output(key("conversation"), Output::Set(key("chat")))
        .output(key("answer"), Output::Set(key("best")))
        .output_end_label(Output::Set(key("label")));
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("duo"))
        .subgraph(nid("duo"), call)
        .edge(
            nid("duo"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .build()
        .unwrap();
    let state = graph
        .start_state([(key("chat"), Value::conversation(question("explain")))])
        .unwrap();
    let state = run_caller(&graph, state).await;
    assert_eq!(state.str(&key("label")).unwrap(), "validated");
    assert_eq!(
        shape(state.conversation(&key("best")).unwrap()),
        ["writer:b"]
    );
    assert_eq!(shape_of(&state, "chat").len(), 4);
}

#[tokio::test]
async fn given_the_duo_as_a_map_body_when_run_then_each_question_gets_its_answer_in_order() {
    let generator_model = Arc::new(LoggingModel::new(vec![
        text_step("one"),
        text_step("two"),
        text_step("two, revised"),
    ]));
    let critic_model = Arc::new(LoggingModel::new(vec![
        verdict(true, ""),
        verdict(false, "revise"),
        verdict(true, ""),
    ]));
    let body = duo_call(
        duo(generator_model, critic_model).graph().unwrap(),
        "question",
    )
    .output(key("answer"), Output::Append(key("answers")))
    .output(key("validated"), Output::Append(key("verdicts")));
    let map = Map {
        list: key("questions"),
        item: key("question"),
        body: Box::new(body),
        max_concurrency: Some(limit(1)),
        on_item_failure: ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("questions"))
        .build()
        .unwrap();
    let questions = Value::list(vec![
        Value::conversation(question("first?")),
        Value::conversation(question("second?")),
    ]);
    let state = graph.start_state([(key("questions"), questions)]).unwrap();
    let state = run_caller(&graph, state).await;
    let answers: Vec<Vec<String>> = state
        .list(&key("answers"))
        .unwrap()
        .iter()
        .map(|value| match value {
            Value::Conversation(conversation) => shape(conversation),
            other => panic!("expected a conversation, got {other}"),
        })
        .collect();
    assert_eq!(answers, [vec!["writer:one"], vec!["writer:two, revised"]]);
    assert_eq!(
        state.list(&key("verdicts")).unwrap(),
        &[Value::bool(true), Value::bool(true)]
    );
}
