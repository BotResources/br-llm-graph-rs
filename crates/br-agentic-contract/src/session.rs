use br_llm_messages::{
    AgentId, Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, Turn,
    TurnClosure, TurnEntry,
};

use crate::error::PortError;
use crate::ids::ThreadRef;
use crate::rules::{
    SessionError, apply_closure, apply_entry, apply_input_turn, apply_response, apply_results,
};
use crate::stream::{ResponseStream, ResultStreamBox};
use crate::thread_access::{AiTurnRef, ThreadAccess};

pub struct Session<'a> {
    access: &'a dyn ThreadAccess,
    thread: ThreadRef,
    conversation: Conversation,
    responses: Vec<EntryId>,
    results: Vec<EntryId>,
}

impl<'a> Session<'a> {
    pub async fn start(access: &'a dyn ThreadAccess, thread: ThreadRef) -> Result<Self, PortError> {
        let conversation = access.read(&thread).await?;
        Ok(Self {
            access,
            thread,
            conversation,
            responses: Vec::new(),
            results: Vec::new(),
        })
    }

    pub fn thread(&self) -> &ThreadRef {
        &self.thread
    }

    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    pub fn open_objects(&self) -> usize {
        self.responses.len() + self.results.len()
    }

    pub fn holds(&self, entry: EntryId) -> bool {
        self.conversation.entry(&entry).is_ok()
    }

    pub fn holds_incoming(&self, entry: EntryId) -> bool {
        use br_llm_messages::EntryRef;
        match self.conversation.entry(&entry) {
            Ok(EntryRef::Incoming { .. }) => true,
            Ok(EntryRef::Turn(held)) => held.is_incoming(),
            _ => false,
        }
    }

    pub fn open_ai_turn(&self, agent: AgentId) -> Option<AiTurnRef> {
        self.conversation
            .turns()
            .iter()
            .rev()
            .filter_map(Turn::as_ai)
            .find(|ai| ai.agent() == agent && !ai.is_closed())
            .map(|ai| AiTurnRef {
                id: ai.id(),
                agent,
                author: ai.author().clone(),
            })
    }

    pub async fn add_input_turn(&mut self, turn: InputTurn) -> Result<(), SessionError> {
        apply_input_turn(&mut self.conversation, &turn)?;
        Ok(self.access.add_input_turn(&self.thread, turn).await?)
    }

    pub async fn add_entry(
        &mut self,
        turn: &AiTurnRef,
        entry: TurnEntry,
    ) -> Result<(), SessionError> {
        apply_entry(&mut self.conversation, turn, entry.clone())?;
        Ok(self
            .access
            .add_entry(&self.thread, turn.clone(), entry)
            .await?)
    }

    pub async fn close_turn(
        &mut self,
        turn: &AiTurnRef,
        closure: TurnClosure,
    ) -> Result<(), SessionError> {
        apply_closure(&mut self.conversation, turn, closure.clone())?;
        Ok(self
            .access
            .close_turn(&self.thread, turn.clone(), closure)
            .await?)
    }

    pub async fn open_response(
        &mut self,
        turn: &AiTurnRef,
        entry: EntryId,
    ) -> Result<ResponseStream, SessionError> {
        let stream = self
            .access
            .open_response(&self.thread, turn.clone(), entry)
            .await?;
        track(&mut self.responses, entry);
        Ok(stream)
    }

    pub async fn complete_response(
        &mut self,
        turn: &AiTurnRef,
        response: ModelResponse,
    ) -> Result<(), SessionError> {
        let entry = response.id();
        if let Err(refusal) = apply_response(&mut self.conversation, turn, response.clone()) {
            self.discard_response_if_open(entry).await;
            return Err(refusal);
        }
        self.access
            .complete_response(&self.thread, turn.clone(), response)
            .await?;
        self.responses.retain(|open| *open != entry);
        Ok(())
    }

    pub async fn discard_response(&mut self, entry: EntryId) -> Result<(), PortError> {
        self.responses.retain(|open| *open != entry);
        self.access.discard_response(&self.thread, entry).await
    }

    pub async fn open_result(
        &mut self,
        turn: &AiTurnRef,
        entry: EntryId,
    ) -> Result<ResultStreamBox, SessionError> {
        let stream = self
            .access
            .open_result(&self.thread, turn.clone(), entry)
            .await?;
        track(&mut self.results, entry);
        Ok(stream)
    }

    pub async fn complete_result(
        &mut self,
        turn: &AiTurnRef,
        result: RuntimeToolResults,
    ) -> Result<(), SessionError> {
        let entry = result.id();
        if let Err(refusal) = apply_results(&mut self.conversation, turn, result.clone()) {
            self.discard_result_if_open(entry).await;
            return Err(refusal);
        }
        self.access
            .complete_result(&self.thread, turn.clone(), result)
            .await?;
        self.results.retain(|open| *open != entry);
        Ok(())
    }

    pub async fn discard_result(&mut self, entry: EntryId) -> Result<(), PortError> {
        self.results.retain(|open| *open != entry);
        self.access.discard_result(&self.thread, entry).await
    }

    pub async fn discard_all(&mut self) -> Vec<PortError> {
        let mut errors = Vec::new();
        for entry in std::mem::take(&mut self.responses) {
            if let Err(error) = self.access.discard_response(&self.thread, entry).await {
                errors.push(error);
            }
        }
        for entry in std::mem::take(&mut self.results) {
            if let Err(error) = self.access.discard_result(&self.thread, entry).await {
                errors.push(error);
            }
        }
        errors
    }

    async fn discard_response_if_open(&mut self, entry: EntryId) {
        if self.responses.contains(&entry) {
            let _ = self.discard_response(entry).await;
        }
    }

    async fn discard_result_if_open(&mut self, entry: EntryId) {
        if self.results.contains(&entry) {
            let _ = self.discard_result(entry).await;
        }
    }
}

fn track(open: &mut Vec<EntryId>, entry: EntryId) {
    if !open.contains(&entry) {
        open.push(entry);
    }
}
