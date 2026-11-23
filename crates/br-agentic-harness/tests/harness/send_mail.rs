use br_agentic_harness::SEND_MAIL;
use br_llm_messages::AgentId;

use crate::common::*;

async fn send(
    world: &World,
    json: &str,
) -> Result<br_agentic_contract::ToolContent, Box<dyn std::error::Error>> {
    let harness = world.full().build();
    let tools = offered(&harness, world).await?;
    call(&find(&tools, SEND_MAIL)?, json).await
}

fn sent(world: &World) -> Vec<br_agentic_contract::memory::SentMail> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            br_agentic_harness_contract::memory::HarnessCall::MailSend { mail, .. } => {
                world.mail.incoming().mail(*mail)
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn given_a_send_with_to_cc_subject_body_and_attachments_when_the_tool_runs_then_the_mail_port_gets_one_mail_with_all_of_them()
-> Res {
    let world = World::new()?;
    let (to, cc) = (agent_id()?, agent_id()?);
    let json = format!(
        "{{\"to\":[\"{}\"],\"cc\":[\"{}\"],\"subject\":\"s\",\"body\":\"b\",\"attachments\":[{{\"name\":\"n.txt\",\"document\":\"doc-1\"}}]}}",
        to.as_uuid(),
        cc.as_uuid()
    );

    let result = send(&world, &json).await?;

    assert!(!result.is_error());
    let mails = sent(&world);
    assert_eq!(mails.len(), 1);
    let mail = &mails[0].mail;
    assert_eq!((mail.to.clone(), mail.cc.clone()), (vec![to], vec![cc]));
    assert_eq!((mail.subject.as_str(), mail.body.as_str()), ("s", "b"));
    assert_eq!(mail.attachments.len(), 1);
    assert_eq!(mail.attachments[0].name(), "n.txt");
    assert_eq!(mail.attachments[0].document().as_str(), "doc-1");
    assert!(text_of(&result).contains(&mail.id.to_string()));
    Ok(())
}

#[tokio::test]
async fn given_a_send_when_the_tool_runs_then_the_author_the_sender_and_the_source_conversation_come_from_the_run_context_and_not_from_the_thread()
-> Res {
    let world = World::new()?;
    let to = agent_id()?;
    let json = format!(
        "{{\"to\":[\"{}\"],\"subject\":\"s\",\"body\":\"b\"}}",
        to.as_uuid()
    );

    send(&world, &json).await?;

    let mails = sent(&world);
    assert_eq!(mails[0].mail.author, world.author);
    assert_eq!(mails[0].sender, world.agent);
    assert_eq!(mails[0].mail.source, world.conversation);
    assert_ne!(mails[0].mail.source.as_uuid(), world.thread.as_uuid());
    Ok(())
}

#[tokio::test]
async fn given_a_send_that_names_an_author_when_the_tool_runs_then_the_author_of_the_run_context_is_used_anyway()
-> Res {
    let world = World::new()?;
    let to = agent_id()?;
    let json = format!(
        "{{\"to\":[\"{}\"],\"subject\":\"s\",\"body\":\"b\",\"author\":\"someone else\",\"source\":\"x\"}}",
        to.as_uuid()
    );

    let result = send(&world, &json).await?;

    assert!(!result.is_error());
    assert_eq!(sent(&world)[0].mail.author, world.author);
    Ok(())
}

#[tokio::test]
async fn given_a_send_with_no_agent_in_to_when_the_tool_runs_then_the_result_is_an_error_and_no_mail_is_sent()
-> Res {
    let world = World::new()?;

    let result = send(&world, "{\"to\":[],\"subject\":\"s\",\"body\":\"b\"}").await?;

    assert!(result.is_error());
    assert!(sent(&world).is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_send_with_a_recipient_twice_when_the_tool_runs_then_the_port_refusal_is_an_error_result()
-> Res {
    let world = World::new()?;
    let to = agent_id()?;
    let json = format!(
        "{{\"to\":[\"{0}\"],\"cc\":[\"{0}\"],\"subject\":\"s\",\"body\":\"b\"}}",
        to.as_uuid()
    );

    let result = send(&world, &json).await?;

    assert!(result.is_error());
    assert!(sent(&world).is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_send_without_a_subject_or_with_a_bad_agent_identity_when_the_tool_runs_then_the_arguments_are_refused_and_nothing_is_sent()
-> Res {
    let world = World::new()?;
    let to = AgentId::from_uuid(uuid())?;

    let no_subject = send(
        &world,
        &format!("{{\"to\":[\"{}\"],\"body\":\"b\"}}", to.as_uuid()),
    )
    .await?;
    let bad_agent = send(
        &world,
        "{\"to\":[\"nobody\"],\"subject\":\"s\",\"body\":\"b\"}",
    )
    .await?;

    assert!(no_subject.is_error());
    assert!(bad_agent.is_error());
    assert!(sent(&world).is_empty());
    Ok(())
}
