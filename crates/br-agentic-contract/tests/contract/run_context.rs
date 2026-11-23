use br_agentic_contract::memory::{Call, FixedToolsPort, Record};
use br_agentic_contract::{RunContext, ToolsPort};

use crate::common::*;

#[test]
fn given_the_five_values_when_a_run_context_is_made_with_its_constructor_then_it_holds_each_of_them()
 {
    let (instance, thread, agent, conversation) =
        (instance_id(), thread_ref(), agent_id(), conversation_id());

    let context = RunContext::new(instance, thread, agent, author("main"), conversation);

    assert_eq!(context.instance, instance);
    assert_eq!(context.thread, thread);
    assert_eq!(context.agent, agent);
    assert_eq!(context.author, author("main"));
    assert_eq!(context.conversation, conversation);
}

#[test]
fn given_a_run_context_when_the_thread_and_the_conversation_differ_then_the_conversation_is_not_the_thread()
 {
    let thread = thread_ref();
    let conversation = conversation_id();

    let context = RunContext::new(
        instance_id(),
        thread,
        agent_id(),
        author("main"),
        conversation,
    );

    assert_ne!(context.thread.as_uuid(), context.conversation.as_uuid());
}

#[tokio::test]
async fn given_a_tools_port_when_it_is_called_with_a_run_context_then_the_record_keeps_that_context_and_the_instance_is_in_it_only()
 {
    let record = Record::new();
    let port = FixedToolsPort::new(&record, Vec::new());
    let context = run_context(thread_ref(), agent_id());

    ToolsPort::<Doc>::tools(&port, &fields(), &context)
        .await
        .unwrap();

    let calls = record.calls();
    let Call::ToolsPort { context: recorded } = &calls[0] else {
        panic!("a tools call")
    };
    assert_eq!(recorded, &context);
    assert_eq!(calls.len(), 1);
}
