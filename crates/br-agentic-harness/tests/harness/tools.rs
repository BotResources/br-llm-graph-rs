use std::sync::Arc;

use br_agentic_contract::{PortError, RunContext};
use br_agentic_harness::{GET_MAIL_FILES, Harness, HarnessHistory, SEND_MAIL};

use crate::common::*;

#[tokio::test]
async fn given_a_runner_tool_named_like_a_mail_tool_when_the_tools_are_built_then_the_harness_refuses_and_names_the_clash()
-> Res {
    for name in [SEND_MAIL, GET_MAIL_FILES] {
        let world = World::new()?;
        let harness = world.full_with(vec![runner_tool(name)?]).build();

        let refused = harness
            .tools()
            .tools(&Doc { n: 0 }, &world.run_context())
            .await;

        match refused {
            Err(PortError::Refused(reason)) => assert!(reason.to_string().contains(name)),
            other => return Err(format!("expected a refusal, got {other:?}").into()),
        }
    }
    Ok(())
}

#[tokio::test]
async fn given_runner_tools_with_other_names_when_the_tools_are_built_then_the_runner_tools_come_first_then_the_two_mail_tools()
-> Res {
    let world = World::new()?;
    let harness = world
        .full_with(vec![runner_tool("lookup")?, runner_tool("search")?])
        .build();

    let tools = offered(&harness, &world).await?;

    let names: Vec<&str> = tools.iter().map(|tool| tool.name().as_str()).collect();
    assert_eq!(names, vec!["lookup", "search", SEND_MAIL, GET_MAIL_FILES]);
    Ok(())
}

#[tokio::test]
async fn given_a_runner_with_no_mail_port_and_a_tool_named_send_mail_when_the_tools_are_built_then_the_tool_of_the_runner_is_kept_and_no_mail_tool_is_added()
-> Res {
    let world = World::new()?;
    let harness = world.builder_with(vec![runner_tool(SEND_MAIL)?]).build();

    let tools = offered(&harness, &world).await?;

    assert_eq!(tools.len(), 1);
    assert!(find(&tools, GET_MAIL_FILES).is_err());
    Ok(())
}

#[tokio::test]
async fn given_a_runner_tools_port_that_faults_when_the_tools_are_built_then_the_harness_returns_the_fault_and_no_tool()
-> Res {
    let world = World::new()?;
    let harness: Harness<Doc> = Harness::builder(
        Arc::new(world.history.clone()) as Arc<dyn HarnessHistory>,
        Arc::new(FixedPrompt(String::new())),
        Arc::new(FaultyTools),
    )
    .mail(Arc::new(world.mail.clone()))
    .build();

    let result = harness
        .tools()
        .tools(&Doc { n: 0 }, &world.run_context())
        .await;

    assert!(matches!(result, Err(PortError::Fault(_))));
    Ok(())
}

#[tokio::test]
async fn given_a_run_context_when_the_tools_are_built_then_the_runner_tools_port_gets_that_same_context_unchanged()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let context = world.run_context();

    harness.tools().tools(&Doc { n: 0 }, &context).await?;

    let seen: Vec<RunContext> = world
        .contract_record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            br_agentic_contract::memory::Call::ToolsPort { context } => Some(context),
            _ => None,
        })
        .collect();
    assert_eq!(seen, vec![context]);
    Ok(())
}

#[tokio::test]
async fn given_two_run_contexts_when_the_tools_are_built_for_each_then_each_set_of_mail_tools_speaks_for_its_own_agent_and_conversation()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let other = RunContext::new(
        instance_id()?,
        world.thread,
        agent_id()?,
        br_llm_messages::Author::new("other")?,
        conversation_id()?,
    );
    let to = agent_id()?;
    let json = format!(
        "{{\"to\":[\"{}\"],\"subject\":\"s\",\"body\":\"b\"}}",
        to.as_uuid()
    );

    let tools = harness.tools().tools(&Doc { n: 0 }, &other).await?;
    call(&find(&tools, SEND_MAIL)?, &json).await?;

    let first = offered(&harness, &world).await?;
    call(&find(&first, SEND_MAIL)?, &json).await?;
    let senders: Vec<_> = world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            br_agentic_harness_contract::memory::HarnessCall::MailSend { context, .. } => {
                Some(context.agent)
            }
            _ => None,
        })
        .collect();
    assert_eq!(senders, vec![other.agent, world.agent]);
    Ok(())
}
