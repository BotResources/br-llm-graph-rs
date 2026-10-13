use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_messages::{
    AssistantBlock, Author, Conversation, Entry, Text, ToolCallId, ToolName, ToolResult,
    ToolResultBlock, Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource, WireMessage,
    WireUserBlock,
};
use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::graph::{
    Always, Context, Count, Graph, GraphBuilder, Input, ItemFailure, Limit, Map, Output, SubGraph,
    Switch, Target,
};
use crate::observe::NoopObserver;
use crate::react::generator_critic::{CriticSeat, GeneratorCritic, GeneratorSeat};
use crate::react::llm_node::Source;
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

pub(crate) fn writer() -> Author {
    Author::new("writer").unwrap()
}

pub(crate) fn reviewer() -> Author {
    Author::new("reviewer").unwrap()
}

fn limit(value: usize) -> Limit {
    Limit::Fixed(NonZeroUsize::new(value).unwrap())
}

pub(crate) fn question(body: &str) -> Conversation {
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

pub(crate) fn verdict(is_valid: bool, message: &str) -> br_llm_messages::Step {
    structured_step(json!({ "thinking": "weighing it", "is_valid": is_valid, "message": message }))
}

pub(crate) fn generator(model: Arc<LoggingModel>) -> GeneratorSeat {
    GeneratorSeat {
        author: writer(),
        model,
        system: Source::Const("Answer the question.".to_owned()),
        output: OutputMode::Text,
        thinking: None,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
    }
}

pub(crate) fn critic(model: Arc<LoggingModel>) -> CriticSeat {
    CriticSeat::new(
        reviewer(),
        model,
        Source::Const("Judge the answer.".to_owned()),
    )
}

pub(crate) fn duo(
    generator_model: Arc<LoggingModel>,
    critic_model: Arc<LoggingModel>,
) -> GeneratorCritic {
    GeneratorCritic::new(
        generator(generator_model),
        critic(critic_model),
        Count::Fixed(3),
    )
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

pub(crate) async fn call_with(
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
        ])
        .unwrap();
    let config = Config::new(graph.schema(), config).unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx(), &mut inbox).await
}

/// A new call on `question("explain")` with the given extra inputs.
pub(crate) async fn start(
    graph: &Graph,
    extra: Vec<(crate::value::Key, Value)>,
) -> Result<Outcome, RunFailure> {
    let mut inputs = vec![
        (
            key("conversation"),
            Value::conversation(question("explain")),
        ),
        (
            key("generator_history"),
            Value::conversation(Conversation::new()),
        ),
    ];
    inputs.extend(extra);
    let state = graph.start_state(inputs).unwrap();
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx(), &mut inbox).await
}

pub(crate) async fn call(graph: &Graph, conversation: Conversation) -> Result<Outcome, RunFailure> {
    call_with(graph, conversation, Conversation::new(), BTreeMap::new()).await
}

pub(crate) fn ended(result: Result<Outcome, RunFailure>, label: &str) -> State {
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
pub(crate) fn shape(conversation: &Conversation) -> Vec<String> {
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

pub(crate) fn shape_of(state: &State, name: &str) -> Vec<String> {
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

pub(crate) fn user_message(text: &str) -> WireMessage {
    WireMessage::User {
        content: vec![WireUserBlock::Text {
            text: Text::new(text).unwrap(),
        }],
    }
}

pub(crate) fn assistant_message(blocks: Vec<AssistantBlock>) -> WireMessage {
    WireMessage::Assistant { content: blocks }
}

pub(crate) fn text_block(text: &str) -> AssistantBlock {
    AssistantBlock::Text {
        text: Text::new(text).unwrap(),
    }
}

pub(crate) fn thinking_block(text: &str) -> AssistantBlock {
    AssistantBlock::Thinking(br_llm_messages::Thinking {
        text: text.to_owned(),
        signature: None,
    })
}

/// An answer whose reasoning comes before its text.
pub(crate) fn thought_step(thinking: &str, text: &str) -> br_llm_messages::Step {
    br_llm_messages::Step::new(
        vec![thinking_block(thinking), text_block(text)],
        br_llm_messages::StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn given_a_rejection_then_a_validation_when_run_then_each_seat_reads_its_own_perspective() {
    let generator_model = Arc::new(LoggingModel::new(vec![
        thought_step("plan one", "first"),
        thought_step("plan two", "second"),
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

    let opening = user_message("<message role=\"user\">explain</message>");
    let critique =
        user_message("<message author=\"reviewer\" role=\"agent\">add an example</message>");
    let generator_requests = generator_model.requests();
    assert_eq!(
        generator_requests[0].system.as_deref(),
        Some("Answer the question.")
    );
    assert_eq!(
        generator_requests[0].messages,
        std::slice::from_ref(&opening)
    );
    assert_eq!(
        generator_requests[1].messages,
        [
            opening.clone(),
            assistant_message(vec![thinking_block("plan one"), text_block("first")]),
            critique.clone(),
        ]
    );

    let critic_requests = critic_model.requests();
    assert_eq!(
        critic_requests[0].system.as_deref(),
        Some("Judge the answer.")
    );
    assert_eq!(
        critic_requests[0].messages,
        [
            opening.clone(),
            user_message(
                "<generator_response>\nfirst\n</generator_response>\n\nPlease evaluate the generator's response above."
            ),
        ]
    );
    assert_eq!(
        critic_requests[1].messages,
        [
            opening,
            assistant_message(vec![text_block("first")]),
            critique,
            user_message(
                "<generator_response>\nsecond\n</generator_response>\n\nPlease evaluate the generator's response above."
            ),
        ]
    );
}

#[tokio::test]
async fn given_every_answer_rejected_with_a_final_generation_when_run_then_exhausted_on_the_unassessed_answer()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("a"), text_step("b")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(false, "fix it")]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        critic(critic_model.clone()),
        Count::Fixed(1),
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
            Count::Fixed(2),
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

fn zero_budget(
    generator_model: Arc<LoggingModel>,
    critic_model: Arc<LoggingModel>,
    max_critiques: Count,
    final_generation: bool,
) -> Arc<Graph> {
    GeneratorCritic {
        final_generation,
        ..GeneratorCritic::new(
            generator(generator_model),
            critic(critic_model),
            max_critiques,
        )
    }
    .graph()
    .unwrap()
}

#[tokio::test]
async fn given_a_critique_budget_of_zero_when_run_then_one_generation_no_critic_and_exhausted() {
    for final_generation in [true, false] {
        let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
        let critic_model = Arc::new(LoggingModel::new(Vec::new()));
        let graph = zero_budget(
            generator_model.clone(),
            critic_model.clone(),
            Count::Fixed(0),
            final_generation,
        );
        let state = ended(call(&graph, question("explain")).await, "exhausted");
        assert!(!state.bool(&key("validated")).unwrap());
        assert_eq!(state.int(&key("generations")).unwrap(), 1);
        assert_eq!(state.str(&key("last_critique")).unwrap(), "");
        assert_eq!(shape_of(&state, "answer"), ["writer:draft"]);
        assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
        assert_eq!(generator_model.requests().len(), 1);
        assert!(critic_model.requests().is_empty());
    }
}

#[tokio::test]
async fn given_histories_ending_on_a_critique_with_a_budget_of_zero_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [("g1", writer(), "a"), ("c1", reviewer(), "fix it")] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_model = Arc::new(LoggingModel::new(Vec::new()));
    let graph = zero_budget(
        generator_model.clone(),
        Arc::new(LoggingModel::new(Vec::new())),
        Count::Fixed(0),
        true,
    );
    let state = ended(
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("generations")).unwrap(), 1);
    assert_eq!(shape_of(&state, "answer"), ["writer:a"]);
    assert!(generator_model.requests().is_empty());
}

#[tokio::test]
async fn given_a_critique_budget_read_from_the_configuration_when_run_then_zero_skips_the_critic_and_one_calls_it()
 {
    let budget = || Count::Config(key("critiques"));
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(Vec::new()));
    let graph = zero_budget(generator_model, critic_model.clone(), budget(), true);
    assert_eq!(
        graph.schema().config.get(&key("critiques")),
        Some(&Kind::Int)
    );
    let config = BTreeMap::from([(key("critiques"), Value::int(0))]);
    let state = ended(
        call_with(&graph, question("explain"), Conversation::new(), config).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("generations")).unwrap(), 1);
    assert!(critic_model.requests().is_empty());

    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let graph = zero_budget(generator_model, critic_model.clone(), budget(), true);
    let config = BTreeMap::from([(key("critiques"), Value::int(1))]);
    ended(
        call_with(&graph, question("explain"), Conversation::new(), config).await,
        "validated",
    );
    assert_eq!(critic_model.requests().len(), 1);
}

#[tokio::test]
async fn given_a_negative_critique_budget_in_the_configuration_when_run_then_count_negative() {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(Vec::new()));
    let graph = zero_budget(
        generator_model,
        critic_model.clone(),
        Count::Config(key("critiques")),
        true,
    );
    let config = BTreeMap::from([(key("critiques"), Value::int(-1))]);
    let failure = call_with(&graph, question("explain"), Conversation::new(), config)
        .await
        .err()
        .unwrap();
    assert!(
        matches!(failure.error, GraphError::CountNegative { value: -1, .. }),
        "{}",
        failure.error
    );
    assert!(critic_model.requests().is_empty());
}

const EMPTY: &str = "The answer was empty. Write a complete answer.";

/// An answer with no text and no structured block: its reasoning only.
pub(crate) fn empty_step() -> br_llm_messages::Step {
    br_llm_messages::Step::new(
        vec![AssistantBlock::Thinking(br_llm_messages::Thinking {
            text: "nothing to say".to_owned(),
            signature: None,
        })],
        br_llm_messages::StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

fn empty_critique() -> String {
    format!("reviewer:{EMPTY}")
}

#[tokio::test]
async fn given_an_empty_answer_then_a_valid_one_when_run_then_the_empty_one_is_rejected_without_the_critic()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![empty_step(), text_step("full")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let graph = GeneratorCritic::new(
        generator(generator_model.clone()),
        critic(critic_model.clone()),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(state.int(&key("generations")).unwrap(), 2);
    assert_eq!(state.str(&key("last_critique")).unwrap(), "");
    let expected = [
        "user".to_owned(),
        "writer:".to_owned(),
        empty_critique(),
        "writer:full".to_owned(),
    ];
    assert_eq!(shape_of(&state, "conversation"), expected);
    assert_eq!(shape_of(&state, "generator_history"), expected);
    assert_eq!(shape_of(&state, "answer"), ["writer:full"]);
    assert_eq!(critic_model.requests().len(), 1);
    assert_eq!(generator_model.requests().len(), 2);
}

#[tokio::test]
async fn given_an_empty_answer_on_the_last_judged_generation_when_run_then_exhausted_on_the_empty_answer_critique()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![empty_step()]));
    let critic_model = Arc::new(LoggingModel::new(Vec::new()));
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(generator_model.clone()),
            critic(critic_model.clone()),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert!(!state.bool(&key("validated")).unwrap());
    assert_eq!(state.int(&key("generations")).unwrap(), 1);
    assert_eq!(state.str(&key("last_critique")).unwrap(), EMPTY);
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user".to_owned(), "writer:".to_owned(), empty_critique()]
    );
    assert!(critic_model.requests().is_empty());
    assert_eq!(generator_model.requests().len(), 1);
}

#[tokio::test]
async fn given_an_empty_unassessed_final_generation_when_run_then_exhausted_without_a_critique() {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("a"), empty_step()]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(false, "fix it")]));
    let graph = GeneratorCritic::new(
        generator(generator_model),
        critic(critic_model.clone()),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert_eq!(state.int(&key("generations")).unwrap(), 2);
    assert_eq!(state.str(&key("last_critique")).unwrap(), "fix it");
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user", "writer:a", "reviewer:fix it", "writer:"]
    );
    assert_eq!(critic_model.requests().len(), 1);
}

#[test]
fn given_a_critic_switch_on_a_key_of_another_kind_when_built_then_refused() {
    let built = GeneratorCritic::new(
        generator(Arc::new(LoggingModel::new(Vec::new()))),
        CriticSeat {
            thinking: Some(Switch::Config(key("rounds"))),
            ..critic(Arc::new(LoggingModel::new(Vec::new())))
        },
        Count::Config(key("rounds")),
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
        Count::Fixed(1),
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
        Count::Fixed(3),
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
async fn given_a_generator_with_an_enabled_subset_when_run_then_only_it_is_offered_and_another_call_is_refused()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "write_file", json!({})),
        text_step("answer"),
    ]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let writer = Arc::new(WriteFileTool::default());
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool), writer.clone()],
            enabled: Some(key("offered")),
            ..generator(generator_model.clone())
        },
        critic(critic_model),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("offered")),
        Some(&Kind::list(Kind::Str))
    );
    let state = ended(
        start(
            &graph,
            vec![(key("offered"), Value::list(vec![Value::str("echo")]))],
        )
        .await,
        "validated",
    );
    let history = state.conversation(&key("generator_history")).unwrap();
    let Some(Entry::Turn(turn)) = history.entries().last() else {
        panic!("expected the generator's turn");
    };
    assert_eq!(
        results_of(turn),
        [(
            "write_file".to_owned(),
            true,
            "Tool write_file is not enabled for this run: this call was not executed.".to_owned()
        )]
    );
    assert_eq!(writer.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    for request in generator_model.requests() {
        let offered: Vec<&str> = request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        assert_eq!(offered, ["echo"]);
    }
}

#[test]
fn given_a_generator_enabled_key_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    let built = GeneratorCritic::new(
        GeneratorSeat {
            enabled: Some(key("answer")),
            ..generator(Arc::new(LoggingModel::new(Vec::new())))
        },
        critic(Arc::new(LoggingModel::new(Vec::new()))),
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(built, Err(GraphError::KeyCollision { .. })));
}

#[tokio::test]
async fn given_a_pass_through_input_when_the_generator_calls_a_tool_then_the_tool_reads_the_value()
{
    let generator_model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "scope", json!({})),
        text_step("answer"),
    ]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let graph = GeneratorCritic {
        inputs: vec![(key("scope"), Kind::Str)],
        ..GeneratorCritic::new(
            GeneratorSeat {
                tools: vec![Arc::new(ScopeTool)],
                ..generator(generator_model)
            },
            critic(critic_model),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("scope")),
        Some(&Kind::Str)
    );
    let state = ended(
        start(&graph, vec![(key("scope"), Value::str("notes/2026"))]).await,
        "validated",
    );
    let history = state.conversation(&key("generator_history")).unwrap();
    let Some(Entry::Turn(turn)) = history.entries().last() else {
        panic!("expected the generator's turn");
    };
    assert_eq!(
        results_of(turn),
        [("scope".to_owned(), false, "scope is notes/2026".to_owned())]
    );
}

#[test]
fn given_a_pass_through_input_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    for name in [
        "conversation",
        "generator_history",
        "answer",
        "validated",
        "generations",
        "last_critique",
    ] {
        let built = GeneratorCritic {
            inputs: vec![(key(name), Kind::Str)],
            ..duo(
                Arc::new(LoggingModel::new(Vec::new())),
                Arc::new(LoggingModel::new(Vec::new())),
            )
        }
        .graph();
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
}

#[tokio::test]
async fn given_histories_ending_on_the_last_allowed_rejection_without_a_final_generation_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [("g1", writer(), "a"), ("c1", reviewer(), "fix it")] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_model = Arc::new(LoggingModel::new(Vec::new()));
    let critic_model = Arc::new(LoggingModel::new(Vec::new()));
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(generator_model.clone()),
            critic(critic_model.clone()),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    let state = ended(
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("generations")).unwrap(), 1);
    assert_eq!(state.str(&key("last_critique")).unwrap(), "fix it");
    assert_eq!(shape_of(&state, "answer"), ["writer:a"]);
    assert!(generator_model.requests().is_empty());
    assert!(critic_model.requests().is_empty());
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
