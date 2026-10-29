use std::sync::Arc;

use br_agentic_contract::{AGENTIC, AgenticNode, Restart};
use br_graph_contract::{CancelView, CreateFuture, Fields, Instance, InstanceId, Node};
use br_llm_messages::Author;

use crate::client::FallbackList;
use crate::config::{Common, Ports, Settings};
use crate::fault::Stop;
use crate::pass::PassEnd;
use crate::ports::Outcome;
use crate::run::{Body, run};
use crate::slot::Slot;
use crate::worker::Worker;

pub const REACT_AGENT: &str = "react_agent";
pub const AGENT_LABEL: &str = "agent";

pub struct ReactAgent<F: Fields> {
    common: Arc<Common<F>>,
    fallback: FallbackList,
    author: Option<Author>,
}

impl<F: Fields> ReactAgent<F> {
    pub fn new(
        name: impl Into<String>,
        ports: Ports<F>,
        fallback: FallbackList,
        settings: Settings,
    ) -> Self {
        Self {
            common: Arc::new(Common {
                name: name.into(),
                ports,
                settings,
            }),
            fallback,
            author: None,
        }
    }

    #[must_use]
    pub fn with_author(mut self, author: Author) -> Self {
        self.author = Some(author);
        self
    }
}

struct ReactBody<F: Fields> {
    common: Arc<Common<F>>,
    fallback: FallbackList,
    author: Option<Author>,
}

impl<F: Fields> Body<F> for ReactBody<F> {
    fn common(&self) -> &Common<F> {
        &self.common
    }

    fn first_label(&self) -> &str {
        AGENT_LABEL
    }

    fn first_author(&self) -> Option<&Author> {
        self.author.as_ref()
    }

    async fn execute(
        &self,
        worker: &mut Worker<'_>,
        snapshot: &F,
        _instance: InstanceId,
    ) -> Result<F::Update, Stop> {
        let mut slot = Slot::new(self.fallback.clone());
        let end = loop {
            let end = worker.pass(&mut slot).await?;
            if matches!(end, PassEnd::Final(_))
                && worker.has_inbox()
                && worker.steps.remaining() > 0
                && worker.drain().await? > 0
            {
                continue;
            }
            break end;
        };
        let reached = end.step_limit_reached();
        let outcome = Outcome::new(end.into_response(), reached, None);
        Ok(worker.end(&self.common, snapshot, outcome).await?)
    }
}

impl<F: Fields> Node<F> for ReactAgent<F> {
    fn name(&self) -> &str {
        &self.common.name
    }

    fn kind(&self) -> &str {
        AGENTIC
    }

    fn subkind(&self) -> &str {
        REACT_AGENT
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, F> {
        let body = Arc::new(ReactBody {
            common: Arc::clone(&self.common),
            fallback: self.fallback.clone(),
            author: self.author.clone(),
        });
        Box::pin(async move {
            let (instance, reader, sender) = Instance::channel(id);
            tokio::spawn(run(body, reader, sender, cancel));
            Ok(instance)
        })
    }
}

impl<F: Fields> AgenticNode<F> for ReactAgent<F> {
    fn has_inbox(&self) -> bool {
        self.common.ports.has_inbox()
    }

    fn restart(&self) -> Restart {
        Restart::Resets
    }
}
