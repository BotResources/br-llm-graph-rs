use br_llm_messages::{
    AgentId, BlockId, EntryId, ModelResponse, RuntimeToolResult, RuntimeToolResults,
};

use crate::fault::ThreadError;
use crate::ids::ThreadRef;
use crate::thread_access::{
    Completed, Opened, ResponseStream, ResultStreamBox, ThreadAccess, TurnTarget,
};

pub struct Session<'a> {
    access: &'a dyn ThreadAccess,
    thread: ThreadRef,
    agent: AgentId,
    responses: Vec<EntryId>,
    results: Vec<EntryId>,
}

impl<'a> Session<'a> {
    pub fn new(access: &'a dyn ThreadAccess, thread: ThreadRef, agent: AgentId) -> Self {
        Self {
            access,
            thread,
            agent,
            responses: Vec::new(),
            results: Vec::new(),
        }
    }

    pub fn open_objects(&self) -> usize {
        self.responses.len() + self.results.len()
    }

    pub async fn open_response(
        &mut self,
        target: TurnTarget,
        entry: EntryId,
    ) -> Result<Opened<ResponseStream, ModelResponse>, ThreadError> {
        let opened = self
            .access
            .open_response(&self.thread, self.agent, target, entry)
            .await?;
        if matches!(opened, Opened::Stream(_)) {
            track(&mut self.responses, entry);
        }
        Ok(opened)
    }

    pub async fn complete_response(
        &mut self,
        target: TurnTarget,
        response: ModelResponse,
    ) -> Result<Completed<ModelResponse>, ThreadError> {
        let entry = response.id();
        let completed = self
            .access
            .complete_response(&self.thread, self.agent, target, response)
            .await?;
        self.responses.retain(|open| *open != entry);
        Ok(completed)
    }

    pub async fn discard_response(&mut self, entry: EntryId) -> Result<(), ThreadError> {
        self.responses.retain(|open| *open != entry);
        self.access
            .discard_response(&self.thread, self.agent, entry)
            .await
    }

    pub async fn open_result(
        &mut self,
        target: TurnTarget,
        entry: EntryId,
        call: BlockId,
    ) -> Result<Opened<ResultStreamBox, RuntimeToolResults>, ThreadError> {
        let opened = self
            .access
            .open_result(&self.thread, self.agent, target, entry, call)
            .await?;
        if matches!(opened, Opened::Stream(_)) {
            track(&mut self.results, entry);
        }
        Ok(opened)
    }

    pub async fn complete_result(
        &mut self,
        target: TurnTarget,
        entry: EntryId,
        result: RuntimeToolResult,
    ) -> Result<Completed<RuntimeToolResults>, ThreadError> {
        let completed = self
            .access
            .complete_result(&self.thread, self.agent, target, entry, result)
            .await?;
        self.results.retain(|open| *open != entry);
        Ok(completed)
    }

    pub async fn discard_result(&mut self, entry: EntryId) -> Result<(), ThreadError> {
        self.results.retain(|open| *open != entry);
        self.access
            .discard_result(&self.thread, self.agent, entry)
            .await
    }

    pub async fn discard_all(&mut self) -> Vec<ThreadError> {
        let mut errors = Vec::new();
        for entry in std::mem::take(&mut self.responses) {
            if let Err(error) = self
                .access
                .discard_response(&self.thread, self.agent, entry)
                .await
            {
                errors.push(error);
            }
        }
        for entry in std::mem::take(&mut self.results) {
            if let Err(error) = self
                .access
                .discard_result(&self.thread, self.agent, entry)
                .await
            {
                errors.push(error);
            }
        }
        errors
    }
}

fn track(open: &mut Vec<EntryId>, entry: EntryId) {
    if !open.contains(&entry) {
        open.push(entry);
    }
}
