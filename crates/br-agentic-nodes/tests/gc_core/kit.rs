use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_nodes::{
    GeneratorCritic, Judgement, Ladder, ModelClient, RoundLimitEnding, StepLimitEnding,
    ThinkingExpectation,
};
use br_graph_contract::Event;
use br_llm_clients_core::{
    BoxFuture, CallOutcome, Client, ClientRequest, ClientSettings, StreamSink,
};
use br_llm_messages::{
    AITurn, ClosureReason, ContentBlock, Conversation, EntryId, ModelResponse, ResponseEnding,
    ResponseMetadata, Turn, TurnClosure, TurnEntry,
};

use crate::common::*;

pub struct Setup {
    pub step_ending: StepLimitEnding,
    pub round_ending: RoundLimitEnding,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            step_ending: StepLimitEnding::WithoutCriticCall,
            round_ending: RoundLimitEnding::FinalGeneration,
        }
    }
}

pub fn build(
    rig: &Rig,
    generator: Ladder,
    critic: Ladder,
    round_limit: u32,
    setup: Setup,
) -> GeneratorCritic<Doc> {
    rig.pair(generator, critic, round_limit)
        .with_step_limit_ending(setup.step_ending)
        .with_round_limit_ending(setup.round_ending)
}

pub fn final_response(text: &str) -> ModelResponse {
    ModelResponse::new(
        entry_id(),
        vec![ContentBlock::Text(
            br_llm_messages::TextBlock::new(block_id(), text).unwrap(),
        )],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )
    .unwrap()
}

pub fn call_response() -> ModelResponse {
    ModelResponse::new(
        entry_id(),
        vec![ContentBlock::RuntimeToolCall(
            br_llm_messages::RuntimeToolCall::new(
                block_id(),
                br_llm_messages::ToolName::new("lookup").unwrap(),
                br_llm_messages::OpaqueJson::parse("{}").unwrap(),
            )
            .unwrap(),
        )],
        ResponseEnding::tool_use(),
        ResponseMetadata::new(),
    )
    .unwrap()
}

pub fn restore(rig: &mut Rig, response: ModelResponse, reason: Option<ClosureReason>) {
    let mut conversation: Conversation = conversation_with(rig.conversation_id, "start");
    let mut turn = AITurn::open(turn_id(), rig.generator, author("Generator"));
    turn.push_entry(TurnEntry::ModelResponse(response)).unwrap();
    if let Some(reason) = reason {
        turn.push_entry(TurnEntry::TurnClosure(TurnClosure::new(
            EntryId::from_uuid(uuid()).unwrap(),
            reason,
        )))
        .unwrap();
    }
    conversation.push_turn(Turn::AITurn(turn)).unwrap();
    rig.access = Arc::new(br_agentic_contract::memory::MemoryThreadAccess::new(
        &rig.record,
        rig.thread,
        conversation,
    ));
}

pub fn restore_final(rig: &mut Rig, text: &str) {
    restore(rig, final_response(text), Some(ClosureReason::completed()));
}

pub fn restore_exhausted(rig: &mut Rig) {
    let reason = ClosureReason::budget_exhausted()
        .with_detail("the step limit was reached")
        .unwrap();
    restore(rig, call_response(), Some(reason));
}

pub fn restore_after_critic(rig: &mut Rig, draft: &str, message: &str) {
    restore_final(rig, draft);
    let mut conversation = rig.conversation();
    let mut turn = AITurn::open(turn_id(), rig.critic, author("Critic"));
    turn.push_entry(TurnEntry::ModelResponse(final_response(message)))
        .unwrap();
    turn.push_entry(TurnEntry::TurnClosure(TurnClosure::new(
        EntryId::from_uuid(uuid()).unwrap(),
        ClosureReason::completed(),
    )))
    .unwrap();
    conversation.push_turn(Turn::AITurn(turn)).unwrap();
    rig.access = Arc::new(br_agentic_contract::memory::MemoryThreadAccess::new(
        &rig.record,
        rig.thread,
        conversation,
    ));
}

pub async fn outcome_of(node: &GeneratorCritic<Doc>) -> Recorded {
    Driver::result(node).await
}

pub fn judgement_of(rig: &Rig) -> Judgement {
    rig.outcome.pair_outcomes().last().unwrap().judgement()
}

pub async fn failed(node: &GeneratorCritic<Doc>) -> String {
    Driver::failure(node).await
}

pub async fn event_of(node: &GeneratorCritic<Doc>) -> Event<Doc> {
    Driver::run(node).await.1
}

pub struct Yielding {
    inner: Arc<Scripted>,
}

impl Client for Yielding {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        Box::pin(async move {
            let outcome = self.inner.call(request, sink).await;
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
            outcome
        })
    }

    fn settings(&self) -> &ClientSettings {
        self.inner.settings()
    }
}

pub fn yielding(scripted: &Arc<Scripted>) -> Ladder {
    ladder(vec![list_yielding(scripted)])
}

pub fn list_yielding(scripted: &Arc<Scripted>) -> br_agentic_nodes::FallbackList {
    br_agentic_nodes::FallbackList::new(vec![ModelClient::new(
        Arc::new(Yielding {
            inner: scripted.clone(),
        }),
        ThinkingExpectation::NotExpected,
    )])
    .unwrap()
}

pub fn deliver_on(
    scripted: &Arc<Scripted>,
    nth: usize,
    deliver: impl Fn() + Send + Sync + 'static,
) {
    let seen = Arc::new(AtomicUsize::new(0));
    scripted.on_call(move |_| {
        if seen.fetch_add(1, Ordering::SeqCst) + 1 == nth {
            deliver();
        }
    });
}

pub fn rungs(clients: &[&Arc<Scripted>]) -> Ladder {
    ladder(clients.iter().map(|scripted| list(&[*scripted])).collect())
}

pub fn rungs_yielding(clients: &[&Arc<Scripted>]) -> Ladder {
    ladder(
        clients
            .iter()
            .map(|scripted| list_yielding(scripted))
            .collect(),
    )
}

pub fn mixed(entries: &[(&Arc<Scripted>, bool)]) -> Ladder {
    ladder(
        entries
            .iter()
            .map(|(scripted, yields)| {
                if *yields {
                    list_yielding(scripted)
                } else {
                    list(&[*scripted])
                }
            })
            .collect(),
    )
}

pub fn calls_of(clients: &[&Arc<Scripted>]) -> Vec<usize> {
    clients.iter().map(|client| client.calls()).collect()
}
