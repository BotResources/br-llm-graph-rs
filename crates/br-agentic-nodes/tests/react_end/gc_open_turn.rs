use std::sync::Arc;

use br_agentic_contract::PortError;
use br_agentic_contract::PortFuture;
use br_agentic_nodes::{
    AuthorSource, GeneratorCritic, ProgrammaticCritic, ProgrammaticCriticPort,
    ProgrammaticRefusalLimit, ProgrammaticVerdict,
};
use br_llm_messages::{AgentId, ClosureReason, ModelResponse};

use crate::access_spy::{AccessSpy, Action, Op, Rule};
use crate::common::*;
use crate::drive::*;
use crate::kit::Parts;

struct Refuses;

impl ProgrammaticCriticPort<Doc> for Refuses {
    fn check<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _response: &'a ModelResponse,
        _refusals_in_a_row: u32,
    ) -> PortFuture<'a, Result<ProgrammaticVerdict, PortError>> {
        Box::pin(async { Ok(ProgrammaticVerdict::Refuse("too long by 3".to_owned())) })
    }
}

fn rule(op: Op, action: Action, agent: AgentId) -> Rule {
    Rule {
        op,
        action,
        agent: Some(agent),
        nth: None,
    }
}

fn pair_over(
    rig: &Rig,
    spy: &Arc<AccessSpy>,
    generator: &Arc<Scripted>,
    critic: &Arc<Scripted>,
    programmatic: bool,
) -> GeneratorCritic<Doc> {
    let node = GeneratorCritic::new(
        "pair",
        Parts::of(rig).with_access(spy.clone()).ports(),
        rig.settings.clone(),
        rig.pair_spec(one(generator), one(critic), 3),
    );
    if !programmatic {
        return node;
    }
    node.with_programmatic_critic(ProgrammaticCritic {
        port: Arc::new(Refuses),
        refusal_limit: ProgrammaticRefusalLimit::new(3).unwrap(),
        author: AuthorSource::Fixed(author("Checker")),
    })
}

fn failed_with(reason: &ClosureReason, needle: &str) -> bool {
    matches!(reason, ClosureReason::Failed { .. })
        && reason
            .detail()
            .is_some_and(|detail| detail.contains(needle))
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_message_whose_storage_fails_when_the_pair_runs_then_the_open_turn_of_the_critic_closes_failed_with_the_reason()
 {
    let rig = Rig::new();
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).with_rule(rule(
        Op::CompleteResponse,
        Action::Fail,
        rig.critic,
    )));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(false, "needs work")]);
    let node = pair_over(&rig, &spy, &generator, &critic, false);

    let (driver, event) = Driver::run(&node).await;

    assert!(is_failed(&event), "{event:?}");
    let closures = spy.closures(rig.critic);
    assert_eq!(closures.len(), 1);
    assert!(
        failed_with(&closures[0], "CompleteResponse failed"),
        "{closures:?}"
    );
    let generator_closures = spy.closures(rig.generator);
    assert_eq!(generator_closures.len(), 1);
    assert!(matches!(
        generator_closures[0],
        ClosureReason::Completed { .. }
    ));
    assert!(input_is_closed(&driver));
    assert_eq!(rig.outcome.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_message_whose_storage_hangs_when_the_pair_is_cancelled_then_the_open_turn_of_the_critic_closes_cancelled()
 {
    let rig = Rig::new();
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).with_rule(rule(
        Op::CompleteResponse,
        Action::Hang,
        rig.critic,
    )));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(false, "needs work")]);
    let node = pair_over(&rig, &spy, &generator, &critic, false);
    let mut driver = Driver::start(&node).await;
    driver.give();
    let critic_agent = rig.critic;

    until(|| {
        spy.log()
            .iter()
            .any(|hand| hand.op == Op::CompleteResponse && hand.agent == Some(critic_agent))
    })
    .await;
    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(is_cancelled(&event), "{event:?}");
    let closures = spy.closures(rig.critic);
    assert_eq!(closures.len(), 1);
    assert!(
        matches!(closures[0], ClosureReason::Cancelled { .. }),
        "{closures:?}"
    );
    assert!(input_is_closed(&driver));
    assert_eq!(rig.outcome.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_programmatic_message_whose_storage_fails_when_the_pair_runs_then_the_open_turn_of_the_programmatic_critic_closes_failed()
 {
    let rig = Rig::new();
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).with_rule(rule(
        Op::CompleteResponse,
        Action::Fail,
        rig.programmatic,
    )));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![]);
    let node = pair_over(&rig, &spy, &generator, &critic, true);

    let event = Driver::run(&node).await.1;

    assert!(is_failed(&event), "{event:?}");
    let closures = spy.closures(rig.programmatic);
    assert_eq!(closures.len(), 1);
    assert!(
        failed_with(&closures[0], "CompleteResponse failed"),
        "{closures:?}"
    );
    assert_eq!(critic.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_programmatic_message_whose_storage_hangs_when_the_pair_is_cancelled_then_the_open_turn_of_the_programmatic_critic_closes_cancelled()
 {
    let rig = Rig::new();
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).with_rule(rule(
        Op::CompleteResponse,
        Action::Hang,
        rig.programmatic,
    )));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![]);
    let node = pair_over(&rig, &spy, &generator, &critic, true);
    let mut driver = Driver::start(&node).await;
    driver.give();
    let programmatic = rig.programmatic;

    until(|| {
        spy.log()
            .iter()
            .any(|hand| hand.op == Op::CompleteResponse && hand.agent == Some(programmatic))
    })
    .await;
    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(is_cancelled(&event), "{event:?}");
    let closures = spy.closures(rig.programmatic);
    assert_eq!(closures.len(), 1);
    assert!(
        matches!(closures[0], ClosureReason::Cancelled { .. }),
        "{closures:?}"
    );
    assert_eq!(critic.calls(), 0);
}
