mod answer;
mod critic;
mod execute;

use std::sync::Arc;

use br_agentic_contract::{AGENTIC, AgenticNode};
use br_graph_contract::{CancelView, CreateFuture, Fields, Instance, InstanceId, Node};

use crate::client::Ladder;
use crate::config::{AuthorSource, Common, Ports, Settings};
use crate::limits::{ProgrammaticRefusalLimit, RoundLimit};
use crate::ports::{
    CriticRequest, CriticRequestPort, GeneratorCriticOutcomePort, ProgrammaticCriticPort,
};
use crate::run::run;

pub const GENERATOR_CRITIC: &str = "generator_critic";
pub const GENERATOR_LABEL: &str = "generator";
pub const CRITIC_LABEL: &str = "critic";
pub const PROGRAMMATIC_CRITIC_LABEL: &str = "programmatic_critic";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepLimitEnding {
    #[default]
    WithoutCriticCall,
    WithCriticCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundLimitEnding {
    #[default]
    FinalGeneration,
    KeepRejectedGeneration,
    Fail,
}

pub enum CriticRequestSource<F: Fields> {
    Fixed(CriticRequest),
    Port(Arc<dyn CriticRequestPort<F>>),
}

pub struct ProgrammaticCritic<F: Fields> {
    pub port: Arc<dyn ProgrammaticCriticPort<F>>,
    pub refusal_limit: ProgrammaticRefusalLimit,
    pub author: AuthorSource<F>,
}

pub struct GeneratorCriticSpec<F: Fields> {
    pub generator: Ladder,
    pub critic: Ladder,
    pub round_limit: RoundLimit,
    pub critic_request: CriticRequestSource<F>,
    pub generator_author: AuthorSource<F>,
    pub critic_author: AuthorSource<F>,
    pub outcome: Arc<dyn GeneratorCriticOutcomePort<F>>,
}

pub(crate) struct GcBody<F: Fields> {
    pub(crate) common: Common<F>,
    pub(crate) spec: GeneratorCriticSpec<F>,
    pub(crate) step_ending: StepLimitEnding,
    pub(crate) round_ending: RoundLimitEnding,
    pub(crate) programmatic: Option<ProgrammaticCritic<F>>,
}

pub struct GeneratorCritic<F: Fields> {
    body: Arc<GcBody<F>>,
}

impl<F: Fields> GeneratorCritic<F> {
    pub fn new(
        name: impl Into<String>,
        ports: Ports<F>,
        settings: Settings,
        spec: GeneratorCriticSpec<F>,
    ) -> Self {
        Self {
            body: Arc::new(GcBody {
                common: Common {
                    name: name.into(),
                    ports,
                    settings,
                },
                spec,
                step_ending: StepLimitEnding::default(),
                round_ending: RoundLimitEnding::default(),
                programmatic: None,
            }),
        }
    }

    fn edit(mut self, change: impl FnOnce(&mut GcBody<F>)) -> Self {
        if let Some(body) = Arc::get_mut(&mut self.body) {
            change(body);
        }
        self
    }

    #[must_use]
    pub fn with_step_limit_ending(self, ending: StepLimitEnding) -> Self {
        self.edit(|body| body.step_ending = ending)
    }

    #[must_use]
    pub fn with_round_limit_ending(self, ending: RoundLimitEnding) -> Self {
        self.edit(|body| body.round_ending = ending)
    }

    #[must_use]
    pub fn with_programmatic_critic(self, critic: ProgrammaticCritic<F>) -> Self {
        self.edit(|body| body.programmatic = Some(critic))
    }
}

impl<F: Fields> Node<F> for GeneratorCritic<F> {
    fn name(&self) -> &str {
        &self.body.common.name
    }

    fn kind(&self) -> &str {
        AGENTIC
    }

    fn subkind(&self) -> &str {
        GENERATOR_CRITIC
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

impl<F: Fields> AgenticNode<F> for GeneratorCritic<F> {
    fn has_inbox(&self) -> bool {
        self.body.common.ports.has_inbox()
    }
}
