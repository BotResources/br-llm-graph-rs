use std::sync::Arc;

use br_agentic_contract::memory::{FixedThreadPort, FixedToolsPort};
use br_agentic_nodes::{
    AuthorSource, CriticRequest, CriticRequestSource, GeneratorCritic, Ladder, Ports,
    ProgrammaticCritic, ProgrammaticRefusalLimit, RoundLimitEnding, StepLimitEnding,
};
use br_llm_messages::AgentId;

use crate::ports::{Checker, Requester};
use crate::prelude::{Doc, Rig, Scripted, author, human_id, one};

pub struct Pair<'r> {
    rig: &'r Rig,
    generator: Ladder,
    critic: Ladder,
    round_limit: u32,
    request: Option<CriticRequestSource<Doc>>,
    programmatic: Option<(Arc<Checker>, u32)>,
    round_ending: RoundLimitEnding,
    step_ending: StepLimitEnding,
    ports: Option<Ports<Doc>>,
}

impl<'r> Pair<'r> {
    pub fn new(rig: &'r Rig, generator: &Arc<Scripted>, critic: &Arc<Scripted>) -> Self {
        Self::ladders(rig, one(generator), one(critic))
    }

    pub fn ladders(rig: &'r Rig, generator: Ladder, critic: Ladder) -> Self {
        Self {
            rig,
            generator,
            critic,
            round_limit: 5,
            request: None,
            programmatic: None,
            round_ending: RoundLimitEnding::default(),
            step_ending: StepLimitEnding::default(),
            ports: None,
        }
    }

    pub fn round_limit(mut self, limit: u32) -> Self {
        self.round_limit = limit;
        self
    }

    pub fn request(mut self, text: &str) -> Self {
        self.request = Some(CriticRequestSource::Fixed(CriticRequest {
            text: text.to_owned(),
            human: human_id(),
        }));
        self
    }

    pub fn request_from(mut self, request: CriticRequest) -> Self {
        self.request = Some(CriticRequestSource::Fixed(request));
        self
    }

    pub fn request_port(mut self, port: &Arc<Requester>) -> Self {
        self.request = Some(CriticRequestSource::Port(port.clone()));
        self
    }

    pub fn programmatic(mut self, checker: &Arc<Checker>, refusal_limit: u32) -> Self {
        self.programmatic = Some((checker.clone(), refusal_limit));
        self
    }

    pub fn round_ending(mut self, ending: RoundLimitEnding) -> Self {
        self.round_ending = ending;
        self
    }

    pub fn step_ending(mut self, ending: StepLimitEnding) -> Self {
        self.step_ending = ending;
        self
    }

    pub fn ports(mut self, ports: Ports<Doc>) -> Self {
        self.ports = Some(ports);
        self
    }

    pub fn build(self) -> GeneratorCritic<Doc> {
        let mut spec = self
            .rig
            .pair_spec(self.generator, self.critic, self.round_limit);
        if let Some(request) = self.request {
            spec.critic_request = request;
        }
        let ports = self.ports.unwrap_or_else(|| self.rig.ports());
        let mut node = GeneratorCritic::new("pair", ports, self.rig.settings.clone(), spec)
            .with_round_limit_ending(self.round_ending)
            .with_step_limit_ending(self.step_ending);
        if let Some((checker, limit)) = self.programmatic {
            node = node.with_programmatic_critic(ProgrammaticCritic {
                port: checker,
                refusal_limit: ProgrammaticRefusalLimit::new(limit).unwrap(),
                author: AuthorSource::Fixed(author("Checker")),
            });
        }
        node
    }
}

pub fn ports_over(rig: &Rig, access: Arc<dyn br_agentic_contract::ThreadAccess>) -> Ports<Doc> {
    Ports::new(
        Arc::new(FixedThreadPort::new(&rig.record, rig.thread)),
        rig.agents.clone(),
        access,
        rig.prompts.clone(),
        Arc::new(FixedToolsPort::new(&rig.record, rig.tools.clone())),
        rig.workspace.clone(),
    )
}

pub fn turns_of(rig: &Rig, agent: AgentId) -> Vec<br_llm_messages::AITurn> {
    rig.conversation()
        .turns()
        .iter()
        .filter_map(|turn| turn.as_ai())
        .filter(|turn| turn.agent() == agent)
        .cloned()
        .collect()
}

pub fn calls_for(rig: &Rig, agent: br_llm_messages::AgentId) -> Vec<&'static str> {
    rig.record
        .calls()
        .iter()
        .filter_map(|call| match call {
            br_agentic_contract::memory::Call::OpenResponse { turn, .. } if turn.agent == agent => {
                Some("open_response")
            }
            br_agentic_contract::memory::Call::CompleteResponse { turn, .. }
                if turn.agent == agent =>
            {
                Some("complete_response")
            }
            br_agentic_contract::memory::Call::AddEntry { turn, .. } if turn.agent == agent => {
                Some("add_entry")
            }
            br_agentic_contract::memory::Call::CloseTurn { turn, .. } if turn.agent == agent => {
                Some("close_turn")
            }
            _ => None,
        })
        .collect()
}

pub fn closures_of(rig: &Rig, agent: AgentId) -> Vec<br_llm_messages::ClosureReason> {
    rig.record
        .calls()
        .iter()
        .filter_map(|call| match call {
            br_agentic_contract::memory::Call::CloseTurn { turn, closure, .. }
                if turn.agent == agent =>
            {
                Some(closure.reason().clone())
            }
            _ => None,
        })
        .collect()
}
