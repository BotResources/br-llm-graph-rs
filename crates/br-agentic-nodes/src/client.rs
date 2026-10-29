use std::sync::Arc;

use br_llm_clients_core::Client;

#[derive(Clone)]
pub struct ModelClient {
    client: Arc<dyn Client>,
    expects_thinking: bool,
}

impl ModelClient {
    pub fn new(client: Arc<dyn Client>) -> Self {
        Self {
            client,
            expects_thinking: false,
        }
    }

    #[must_use]
    pub fn expecting_thinking(mut self) -> Self {
        self.expects_thinking = true;
        self
    }

    pub fn client(&self) -> &dyn Client {
        &*self.client
    }

    pub fn should_have_thought(&self) -> bool {
        self.expects_thinking
    }
}

impl std::fmt::Debug for ModelClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelClient")
            .field("expects_thinking", &self.expects_thinking)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct FallbackList {
    clients: Vec<ModelClient>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a fallback list or a ladder holds at least one entry")]
pub struct EmptyList;

impl FallbackList {
    pub fn new(clients: Vec<ModelClient>) -> Result<Self, EmptyList> {
        if clients.is_empty() {
            return Err(EmptyList);
        }
        Ok(Self { clients })
    }

    pub fn len(&self) -> usize {
        self.clients.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    pub(crate) fn get(&self, index: usize) -> Option<&ModelClient> {
        self.clients.get(index)
    }
}

impl From<ModelClient> for FallbackList {
    fn from(client: ModelClient) -> Self {
        Self {
            clients: vec![client],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Ladder {
    entries: Vec<FallbackList>,
}

impl Ladder {
    pub fn new(entries: Vec<FallbackList>) -> Result<Self, EmptyList> {
        if entries.is_empty() {
            return Err(EmptyList);
        }
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, position: u32) -> &FallbackList {
        let last = self.entries.len().saturating_sub(1);
        let index = usize::try_from(position).map_or(last, |index| index.min(last));
        match self.entries.get(index) {
            Some(entry) => entry,
            None => &self.entries[last],
        }
    }
}
