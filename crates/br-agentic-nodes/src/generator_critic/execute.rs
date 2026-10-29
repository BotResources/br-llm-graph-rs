use br_graph_contract::{Fields, InstanceId};
use br_llm_messages::Author;

use super::answer::{Answer, critic_shape};
use super::critic::{Seat, call_critic, store_message};
use super::{CRITIC_LABEL, CriticRequestSource, GENERATOR_LABEL, GcBody, StepLimitEnding};
use crate::config::Common;
use crate::fault::{Fault, Stop};
use crate::pass::PassEnd;
use crate::ports::Outcome;
use crate::run::Body;
use crate::slot::Slot;
use crate::start::{agent_label, author_of};
use crate::worker::Worker;

struct Rounds {
    unvalidated: u32,
}

impl Rounds {
    fn position(&self) -> u32 {
        self.unvalidated
    }

    fn reset(&mut self) {
        self.unvalidated = 0;
    }

    fn up(&mut self) {
        self.unvalidated = self.unvalidated.saturating_add(1);
    }
}

impl<F: Fields> GcBody<F> {
    async fn seat(&self, snapshot: &F, instance: InstanceId) -> Result<Seat, Fault> {
        let common: &Common<F> = &self.common;
        let label = agent_label(CRITIC_LABEL)?;
        let agent = common.ports.agent.agent(snapshot, instance, &label).await?;
        let author = author_of(self.critic_author.as_ref(), &agent, &label)?;
        let system = common.ports.prompt.prompt(snapshot, &label).await?;
        let request = match &*self.critic_request {
            CriticRequestSource::Fixed(request) => request.clone(),
            CriticRequestSource::Port(port) => port.request(snapshot).await?,
        };
        Ok(Seat {
            agent: agent.id,
            author,
            system,
            request,
        })
    }

    async fn ask(&self, worker: &Worker<'_>, seat: &Seat, position: u32) -> Result<Answer, Fault> {
        let mut slot = Slot::new(self.critic.entry(position).clone());
        let shape = critic_shape(self.thinking_field)?;
        call_critic(
            worker.access,
            &worker.context.thread,
            seat,
            &mut slot,
            &shape,
            worker.timeouts.model_call(),
        )
        .await
    }
}

impl<F: Fields> Body<F> for GcBody<F> {
    fn common(&self) -> &Common<F> {
        &self.common
    }

    fn first_label(&self) -> &str {
        GENERATOR_LABEL
    }

    fn first_author(&self) -> Option<&Author> {
        self.generator_author.as_ref()
    }

    async fn execute(
        &self,
        worker: &mut Worker<'_>,
        snapshot: &F,
        instance: InstanceId,
    ) -> Result<F::Update, Stop> {
        let mut seat: Option<Seat> = None;
        let mut rounds = Rounds { unvalidated: 0 };
        let mut first = true;
        let mut resume = false;
        let (response, reached, verdict) = loop {
            if !first && !resume && worker.drain().await? > 0 {
                rounds.reset();
            }
            first = false;
            resume = false;
            let position = rounds.position();
            let mut slot = Slot::new(self.generator.entry(position).clone());
            let stored_before = worker.stored_items;
            let end = worker.pass(&mut slot).await?;
            if worker.stored_items > stored_before {
                rounds.reset();
            }
            let reached = end.step_limit_reached();
            let stopped_by_limit = matches!(end, PassEnd::StepLimit(_));
            let response = end.into_response();
            if stopped_by_limit && self.ending == StepLimitEnding::WithoutCriticCall {
                break (response, reached, None);
            }
            if seat.is_none() {
                seat = Some(self.seat(snapshot, instance).await?);
            }
            let Some(seat) = seat.as_ref() else {
                return Err(Fault::Value("the critic has no seat".to_owned()).into());
            };
            let answer = self.ask(worker, seat, position).await?;
            if stopped_by_limit {
                if answer.validated {
                    break (response, reached, Some(true));
                }
                return Err(Fault::CriticAtStepLimit {
                    message: answer.message,
                }
                .into());
            }
            if answer.validated {
                if worker.has_inbox() && worker.steps.remaining() > 0 && worker.drain().await? > 0 {
                    rounds.reset();
                    resume = true;
                    continue;
                }
                break (response, reached, Some(true));
            }
            store_message(worker.access, &worker.context.thread, seat, answer.message).await?;
            rounds.up();
            if rounds.unvalidated >= self.round_limit.get() || worker.steps.remaining() == 0 {
                break (response, reached, Some(false));
            }
        };
        let outcome = Outcome::new(response, reached, verdict);
        Ok(worker.end(&self.common, snapshot, outcome).await?)
    }
}
