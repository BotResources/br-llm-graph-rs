use br_graph_contract::Fields;
use br_llm_messages::{ModelResponse, PlainText};

use super::answer::Answer;
use super::critic::{call_critic, store_message};
use super::{
    CRITIC_LABEL, CriticRequestSource, GENERATOR_LABEL, GcBody, PROGRAMMATIC_CRITIC_LABEL,
    RoundLimitEnding, StepLimitEnding,
};
use crate::config::Common;
use crate::drain::Drained;
use crate::fault::{Fault, Stop};
use crate::outcome::{GeneratorCriticOutcome, Judgement};
use crate::pass::PassEnd;
use crate::ports::ProgrammaticVerdict;
use crate::run::Body;
use crate::seats::{SeatSpec, Seats};
use crate::slot::Slot;
use crate::worker::Worker;

struct Finish {
    response: ModelResponse,
    judgement: Judgement,
    step_limit_reached: bool,
}

enum Next {
    Pass,
    End(Box<Finish>),
}

struct Loop {
    iterations: u32,
    drain_first: bool,
    last_scheduled: bool,
    is_last: bool,
    position: u32,
    keep_position: Option<u32>,
}

impl<F: Fields> GcBody<F> {
    async fn ask(
        &self,
        worker: &Worker<'_>,
        seats: &Seats,
        snapshot: &F,
        response: &ModelResponse,
        position: u32,
    ) -> Result<Answer, Fault> {
        let request = match &self.spec.critic_request {
            CriticRequestSource::Fixed(request) => request.clone(),
            CriticRequestSource::Port(port) => port.request(snapshot, response).await?,
        };
        PlainText::new(request.text.as_str()).map_err(Fault::value)?;
        call_critic(
            worker,
            seats.get(CRITIC_LABEL)?,
            &request,
            self.spec.critic.entry(position),
        )
        .await
    }

    async fn programmatic(
        &self,
        worker: &mut Worker<'_>,
        seats: &Seats,
        snapshot: &F,
        response: &ModelResponse,
        state: &mut Loop,
    ) -> Result<bool, Fault> {
        let Some(critic) = &self.programmatic else {
            return Ok(true);
        };
        let verdict = critic
            .port
            .check(snapshot, response, worker.counters.refusals)
            .await?;
        let reason = match verdict {
            ProgrammaticVerdict::Accept => {
                worker.counters.refusals = 0;
                return Ok(true);
            }
            ProgrammaticVerdict::Refuse(reason) => reason,
        };
        if reason.trim().is_empty() {
            return Err(Fault::BlankExplicitReason);
        }
        let seat = seats.get(PROGRAMMATIC_CRITIC_LABEL)?;
        store_message(worker, seat.agent, &seat.author, &reason).await?;
        worker.counters.refusals = worker.counters.refusals.saturating_add(1);
        if worker.counters.refusals >= critic.refusal_limit.get() || worker.steps_remaining() == 0 {
            return Err(Fault::ProgrammaticRefusal { reason });
        }
        state.keep_position = Some(state.position);
        state.drain_first = true;
        Ok(false)
    }

    async fn judge(
        &self,
        worker: &mut Worker<'_>,
        seats: &Seats,
        snapshot: &F,
        end: PassEnd,
        state: &mut Loop,
    ) -> Result<Next, Fault> {
        let reached = end.step_limit_reached();
        let response = end.into_response();
        let finish = |judgement| {
            Next::End(Box::new(Finish {
                response: response.clone(),
                judgement,
                step_limit_reached: reached,
            }))
        };
        if reached {
            if state.is_last || self.step_ending == StepLimitEnding::WithoutCriticCall {
                return Ok(finish(Judgement::NotJudged));
            }
            let answer = self
                .ask(worker, seats, snapshot, &response, state.position)
                .await?;
            return if answer.validated {
                Ok(finish(Judgement::Validated))
            } else {
                Err(Fault::CriticAtStepLimit {
                    message: answer.message,
                })
            };
        }
        if !self
            .programmatic(worker, seats, snapshot, &response, state)
            .await?
        {
            return Ok(Next::Pass);
        }
        if state.is_last {
            return Ok(finish(Judgement::NotJudged));
        }
        let answer = self
            .ask(worker, seats, snapshot, &response, state.position)
            .await?;
        if answer.validated {
            return self
                .validated(worker, state, finish(Judgement::Validated))
                .await;
        }
        self.rejected(worker, seats, answer, state, finish(Judgement::Rejected))
            .await
    }

    async fn validated(
        &self,
        worker: &mut Worker<'_>,
        state: &mut Loop,
        end: Next,
    ) -> Result<Next, Fault> {
        if worker.has_inbox() && worker.steps_remaining() > 0 {
            let drained = worker.drain().await?;
            if drained.stored > 0 {
                state.drain_first = false;
                state.last_scheduled = false;
                state.keep_position = None;
                return Ok(Next::Pass);
            }
        }
        Ok(end)
    }

    async fn rejected(
        &self,
        worker: &mut Worker<'_>,
        seats: &Seats,
        answer: Answer,
        state: &mut Loop,
        end: Next,
    ) -> Result<Next, Fault> {
        worker.counters.rounds = worker.counters.rounds.saturating_add(1);
        let at_limit = worker.counters.rounds >= self.spec.round_limit.get();
        if at_limit && self.round_ending == RoundLimitEnding::Fail {
            return Err(Fault::CriticAtRoundLimit {
                message: answer.message,
            });
        }
        let seat = seats.get(CRITIC_LABEL)?;
        store_message(worker, seat.agent, &seat.author, &answer.message).await?;
        let continues = worker.steps_remaining() > 0;
        if at_limit {
            if self.round_ending == RoundLimitEnding::FinalGeneration && continues {
                state.last_scheduled = true;
                state.drain_first = true;
                return Ok(Next::Pass);
            }
            return Ok(end);
        }
        if !continues {
            return Ok(end);
        }
        state.last_scheduled = false;
        state.drain_first = true;
        Ok(Next::Pass)
    }
}

impl<F: Fields> Body<F> for GcBody<F> {
    fn common(&self) -> &Common<F> {
        &self.common
    }

    fn seats(&self) -> Vec<SeatSpec<'_, F>> {
        let mut seats = vec![
            SeatSpec {
                label: GENERATOR_LABEL,
                author: &self.spec.generator_author,
                prompt: true,
            },
            SeatSpec {
                label: CRITIC_LABEL,
                author: &self.spec.critic_author,
                prompt: true,
            },
        ];
        if let Some(critic) = &self.programmatic {
            seats.push(SeatSpec {
                label: PROGRAMMATIC_CRITIC_LABEL,
                author: &critic.author,
                prompt: false,
            });
        }
        seats
    }

    async fn execute(
        &self,
        worker: &mut Worker<'_>,
        seats: &Seats,
        snapshot: &F,
    ) -> Result<F::Update, Stop> {
        let mut state = Loop {
            iterations: 0,
            drain_first: false,
            last_scheduled: false,
            is_last: false,
            position: worker.counters.rounds,
            keep_position: None,
        };
        let mut ended = worker.restored();
        let finish = loop {
            let end = match ended.take() {
                Some(end) => end,
                None => {
                    let drained = if state.drain_first {
                        worker.drain().await?
                    } else {
                        Drained::default()
                    };
                    state.drain_first = false;
                    if drained.request_stored {
                        state.last_scheduled = false;
                        state.keep_position = None;
                    }
                    state.position = state.keep_position.take().unwrap_or(worker.counters.rounds);
                    state.is_last = state.last_scheduled;
                    let mut slot = Slot::new(self.spec.generator.entry(state.position).clone());
                    state.iterations += 1;
                    worker.pass(&mut slot).await?
                }
            };
            match self.judge(worker, seats, snapshot, end, &mut state).await? {
                Next::Pass => {}
                Next::End(finish) => break *finish,
            }
        };
        let outcome = GeneratorCriticOutcome::new(
            finish.response,
            finish.judgement,
            state.iterations,
            finish.step_limit_reached,
        );
        let update = self
            .spec
            .outcome
            .update(snapshot, &outcome)
            .await
            .map_err(Fault::from)?;
        Ok(worker.finish(&self.common, snapshot, update).await?)
    }
}
