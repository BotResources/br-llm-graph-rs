use br_agentic_contract::{
    AgentLabel, AuthorPort, CriticRequest, CriticRequestPort, GeneratorCriticOutcome,
    GeneratorCriticOutcomePort, Judgement, OutputSchemaPort, PortError, PortFuture,
    ProgrammaticCriticPort, ProgrammaticVerdict, PromptPort, ReactOutcome, ReactOutcomePort,
};
use br_llm_clients_core::StructuredShape;
use br_llm_messages::{HumanId, ModelResponse};

use crate::common::*;

struct Fixed {
    refuse: bool,
}

fn outcome_of(refuse: bool, value: i64) -> Result<i64, PortError> {
    if refuse {
        Err(PortError::refused("no update"))
    } else {
        Ok(value)
    }
}

impl PromptPort<Doc> for Fixed {
    fn prompt<'a>(
        &'a self,
        snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move { Ok(format!("{label}:{}", snapshot.n)) })
    }
}

impl OutputSchemaPort<Doc> for Fixed {
    fn shape<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<Option<StructuredShape>, PortError>> {
        Box::pin(async { Ok(None) })
    }
}

impl AuthorPort<Doc> for Fixed {
    fn author<'a>(
        &'a self,
        _snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move { Ok(format!("author of {label}")) })
    }
}

impl ReactOutcomePort<Doc> for Fixed {
    fn update<'a>(
        &'a self,
        _snapshot: &'a Doc,
        outcome: &'a ReactOutcome,
    ) -> PortFuture<'a, Result<i64, PortError>> {
        Box::pin(async move { outcome_of(self.refuse, i64::from(outcome.steps())) })
    }
}

impl GeneratorCriticOutcomePort<Doc> for Fixed {
    fn update<'a>(
        &'a self,
        _snapshot: &'a Doc,
        outcome: &'a GeneratorCriticOutcome,
    ) -> PortFuture<'a, Result<i64, PortError>> {
        Box::pin(async move { outcome_of(self.refuse, i64::from(outcome.iterations())) })
    }
}

impl CriticRequestPort<Doc> for Fixed {
    fn request<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _response: &'a ModelResponse,
    ) -> PortFuture<'a, Result<CriticRequest, PortError>> {
        Box::pin(async {
            Ok(CriticRequest {
                text: "check it".to_owned(),
                human: HumanId::from_uuid(uuid()).unwrap(),
            })
        })
    }
}

impl ProgrammaticCriticPort<Doc> for Fixed {
    fn check<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _response: &'a ModelResponse,
        refusals_in_a_row: u32,
    ) -> PortFuture<'a, Result<ProgrammaticVerdict, PortError>> {
        Box::pin(async move {
            Ok(if refusals_in_a_row == 0 {
                ProgrammaticVerdict::Accept
            } else {
                ProgrammaticVerdict::Refuse(format!("{refusals_in_a_row} refusals"))
            })
        })
    }
}

fn label() -> AgentLabel {
    AgentLabel::new("critic").unwrap()
}

#[tokio::test]
async fn given_a_snapshot_and_a_label_when_the_prompt_the_shape_and_the_author_are_asked_then_each_answers_for_that_agent()
 {
    let port = Fixed { refuse: false };

    assert_eq!(port.prompt(&fields(), &label()).await.unwrap(), "critic:1");
    assert!(port.shape(&fields(), &label()).await.unwrap().is_none());
    assert_eq!(
        port.author(&fields(), &label()).await.unwrap(),
        "author of critic"
    );
}

#[tokio::test]
async fn given_an_outcome_when_an_outcome_port_is_asked_then_it_gives_the_update_of_the_fields_or_the_one_port_error()
 {
    let react = ReactOutcome::new(response_text(entry_id(), "done"), 4, false);
    let gc = GeneratorCriticOutcome::new(
        response_text(entry_id(), "done"),
        Judgement::Rejected,
        2,
        true,
    );

    assert_eq!(
        ReactOutcomePort::update(&Fixed { refuse: false }, &fields(), &react)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        GeneratorCriticOutcomePort::update(&Fixed { refuse: false }, &fields(), &gc)
            .await
            .unwrap(),
        2
    );
    let error = ReactOutcomePort::update(&Fixed { refuse: true }, &fields(), &react)
        .await
        .unwrap_err();
    assert!(error.is_refusal());
}

#[tokio::test]
async fn given_a_response_when_the_critic_ports_are_asked_then_the_request_holds_text_and_human_and_the_verdict_accepts_or_refuses_with_a_text()
 {
    let port = Fixed { refuse: false };
    let response = response_text(entry_id(), "draft");

    let request = port.request(&fields(), &response).await.unwrap();

    assert_eq!(request.text, "check it");
    assert_eq!(
        port.check(&fields(), &response, 0).await.unwrap(),
        ProgrammaticVerdict::Accept
    );
    assert_eq!(
        port.check(&fields(), &response, 2).await.unwrap(),
        ProgrammaticVerdict::Refuse("2 refusals".to_owned())
    );
}
