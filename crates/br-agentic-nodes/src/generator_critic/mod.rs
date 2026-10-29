mod answer;
mod critic;
mod execute;

use std::sync::Arc;

use br_agentic_contract::{AGENTIC, AgenticNode, Restart};
use br_graph_contract::{CancelView, CreateFuture, Fields, Instance, InstanceId, Node};
use br_llm_messages::Author;

use crate::client::Ladder;
use crate::config::{Common, Ports, Settings};
use crate::limits::RoundLimit;
use crate::ports::{CriticRequest, CriticRequestPort};
use crate::run::run;

pub const GENERATOR_CRITIC: &str = "generator_critic";
pub const GENERATOR_LABEL: &str = "generator";
pub const CRITIC_LABEL: &str = "critic";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepLimitEnding {
    #[default]
    WithoutCriticCall,
    WithCriticCall,
}

pub enum CriticRequestSource<F: Fields> {
    Fixed(CriticRequest),
    Port(Arc<dyn CriticRequestPort<F>>),
}

pub struct GeneratorCriticSpec<F: Fields> {
    pub generator: Ladder,
    pub critic: Ladder,
    pub round_limit: RoundLimit,
    pub critic_request: CriticRequestSource<F>,
}

pub(crate) struct GcBody<F: Fields> {
    pub(crate) common: Arc<Common<F>>,
    pub(crate) generator: Ladder,
    pub(crate) critic: Ladder,
    pub(crate) round_limit: RoundLimit,
    pub(crate) ending: StepLimitEnding,
    pub(crate) critic_request: Arc<CriticRequestSource<F>>,
    pub(crate) generator_author: Option<Author>,
    pub(crate) critic_author: Option<Author>,
    pub(crate) thinking_field: bool,
}

pub struct GeneratorCritic<F: Fields> {
    common: Arc<Common<F>>,
    generator: Ladder,
    critic: Ladder,
    round_limit: RoundLimit,
    ending: StepLimitEnding,
    critic_request: Arc<CriticRequestSource<F>>,
    generator_author: Option<Author>,
    critic_author: Option<Author>,
    thinking_field: bool,
}

impl<F: Fields> GeneratorCritic<F> {
    pub fn new(
        name: impl Into<String>,
        ports: Ports<F>,
        settings: Settings,
        spec: GeneratorCriticSpec<F>,
    ) -> Self {
        Self {
            common: Arc::new(Common {
                name: name.into(),
                ports,
                settings,
            }),
            generator: spec.generator,
            critic: spec.critic,
            round_limit: spec.round_limit,
            ending: StepLimitEnding::default(),
            critic_request: Arc::new(spec.critic_request),
            generator_author: None,
            critic_author: None,
            thinking_field: false,
        }
    }

    #[must_use]
    pub fn with_authors(mut self, generator: Author, critic: Author) -> Self {
        self.generator_author = Some(generator);
        self.critic_author = Some(critic);
        self
    }

    #[must_use]
    pub fn with_generator_author(mut self, author: Author) -> Self {
        self.generator_author = Some(author);
        self
    }

    #[must_use]
    pub fn with_critic_author(mut self, author: Author) -> Self {
        self.critic_author = Some(author);
        self
    }

    #[must_use]
    pub fn with_step_limit_ending(mut self, ending: StepLimitEnding) -> Self {
        self.ending = ending;
        self
    }

    #[must_use]
    pub fn with_critic_thinking_field(mut self) -> Self {
        self.thinking_field = true;
        self
    }
}

impl<F: Fields> Node<F> for GeneratorCritic<F> {
    fn name(&self) -> &str {
        &self.common.name
    }

    fn kind(&self) -> &str {
        AGENTIC
    }

    fn subkind(&self) -> &str {
        GENERATOR_CRITIC
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, F> {
        let body = Arc::new(GcBody {
            common: Arc::clone(&self.common),
            generator: self.generator.clone(),
            critic: self.critic.clone(),
            round_limit: self.round_limit,
            ending: self.ending,
            critic_request: Arc::clone(&self.critic_request),
            generator_author: self.generator_author.clone(),
            critic_author: self.critic_author.clone(),
            thinking_field: self.thinking_field,
        });
        Box::pin(async move {
            let (instance, reader, sender) = Instance::channel(id);
            tokio::spawn(run(body, reader, sender, cancel));
            Ok(instance)
        })
    }
}

impl<F: Fields> AgenticNode<F> for GeneratorCritic<F> {
    fn has_inbox(&self) -> bool {
        self.common.ports.has_inbox()
    }

    fn restart(&self) -> Restart {
        Restart::Resets
    }
}
