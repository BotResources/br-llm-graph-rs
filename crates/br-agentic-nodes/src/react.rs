use std::sync::Arc;

use br_agentic_contract::{AGENTIC, AgenticNode};
use br_graph_contract::{CancelView, CreateFuture, Fields, Instance, InstanceId, Node};

use crate::client::FallbackList;
use crate::config::{AuthorSource, Common, Ports, Settings};
use crate::fault::Stop;
use crate::outcome::ReactOutcome;
use crate::pass::PassEnd;
use crate::ports::ReactOutcomePort;
use crate::run::{Body, run};
use crate::seats::{SeatSpec, Seats};
use crate::slot::Slot;
use crate::worker::Worker;

pub const REACT_AGENT: &str = "react_agent";
pub const AGENT_LABEL: &str = "agent";

pub struct ReactSpec<F: Fields> {
    pub fallback: FallbackList,
    pub outcome: Arc<dyn ReactOutcomePort<F>>,
    pub author: AuthorSource<F>,
}

struct ReactBody<F: Fields> {
    common: Arc<Common<F>>,
    fallback: FallbackList,
    outcome: Arc<dyn ReactOutcomePort<F>>,
    author: AuthorSource<F>,
}

pub struct ReactAgent<F: Fields> {
    body: Arc<ReactBody<F>>,
}

impl<F: Fields> ReactAgent<F> {
    pub fn new(
        name: impl Into<String>,
        ports: Ports<F>,
        settings: Settings,
        spec: ReactSpec<F>,
    ) -> Self {
        Self {
            body: Arc::new(ReactBody {
                common: Arc::new(Common {
                    name: name.into(),
                    ports,
                    settings,
                }),
                fallback: spec.fallback,
                outcome: spec.outcome,
                author: spec.author,
            }),
        }
    }
}

impl<F: Fields> Body<F> for ReactBody<F> {
    fn common(&self) -> &Common<F> {
        &self.common
    }

    fn seats(&self) -> Vec<SeatSpec<'_, F>> {
        vec![SeatSpec {
            label: AGENT_LABEL,
            author: &self.author,
            prompt: true,
        }]
    }

    async fn execute(
        &self,
        worker: &mut Worker<'_>,
        _seats: &Seats,
        snapshot: &F,
    ) -> Result<F::Update, Stop> {
        let mut slot = Slot::new(self.fallback.clone());
        let end = match worker.restored() {
            Some(end) => end,
            None => loop {
                let end = worker.pass(&mut slot).await?;
                if matches!(end, PassEnd::Final(_))
                    && worker.has_inbox()
                    && worker.steps_remaining() > 0
                    && worker.drain().await?.stored > 0
                {
                    continue;
                }
                break end;
            },
        };
        let reached = end.step_limit_reached();
        let outcome = ReactOutcome::new(end.into_response(), worker.steps_made(), reached);
        let update = self
            .outcome
            .update(snapshot, &outcome)
            .await
            .map_err(crate::fault::Fault::from)?;
        Ok(worker.finish(&self.common, snapshot, update).await?)
    }
}

impl<F: Fields> Node<F> for ReactAgent<F> {
    fn name(&self) -> &str {
        &self.body.common.name
    }

    fn kind(&self) -> &str {
        AGENTIC
    }

    fn subkind(&self) -> &str {
        REACT_AGENT
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, F> {
        let body = Arc::clone(&self.body);
        Box::pin(async move {
            let (instance, reader, sender) = Instance::channel(id);
            tokio::spawn(run(body, reader, sender, cancel));
            Ok(instance)
        })
    }
}

impl<F: Fields> AgenticNode<F> for ReactAgent<F> {
    fn has_inbox(&self) -> bool {
        self.body.common.ports.has_inbox()
    }
}
