use br_agentic_contract::memory::{
    Call, FixedThreadPort, FixedToolsPort, MemoryAgentPort, MemoryThreadAccess, MemoryWorkspace,
    Record,
};
use br_agentic_contract::{
    AgentLabel, AgentPort, FileSystemMode, InboxPort, ThreadAccess, ThreadPort, ToolsPort,
};
use br_llm_messages::{Conversation, TurnEntry};

use crate::common::*;

fn label(text: &str) -> AgentLabel {
    AgentLabel::new(text).unwrap()
}

#[tokio::test]
async fn given_an_agent_label_when_the_in_memory_agent_port_is_asked_twice_then_it_gives_the_same_identity()
 {
    let port = MemoryAgentPort::<Doc>::new(&Record::new());

    let first = port
        .agent(&fields(), instance_id(), &label("agent"))
        .await
        .unwrap();
    let again = port
        .agent(&fields(), instance_id(), &label("agent"))
        .await
        .unwrap();

    assert_eq!(first, again);
}

#[tokio::test]
async fn given_two_labels_when_the_in_memory_agent_port_is_asked_then_it_gives_two_identities() {
    let port = MemoryAgentPort::<Doc>::new(&Record::new());

    let generator = port
        .agent(&fields(), instance_id(), &label("generator"))
        .await
        .unwrap();
    let critic = port
        .agent(&fields(), instance_id(), &label("critic"))
        .await
        .unwrap();

    assert_ne!(generator, critic);
}

#[tokio::test]
async fn given_a_new_in_memory_agent_port_when_it_is_asked_for_a_label_then_it_models_a_retried_job_with_a_new_identity()
 {
    let record = Record::new();
    let first = MemoryAgentPort::<Doc>::new(&record);
    let retried = MemoryAgentPort::<Doc>::new(&record);

    let old = first
        .agent(&fields(), instance_id(), &label("agent"))
        .await
        .unwrap();
    let new = retried
        .agent(&fields(), instance_id(), &label("agent"))
        .await
        .unwrap();

    assert_ne!(old, new);
}

#[tokio::test]
async fn given_an_identity_set_by_the_test_when_the_in_memory_agent_port_is_asked_then_it_gives_that_identity()
 {
    let chosen = agent_id();
    let port = MemoryAgentPort::<Doc>::new(&Record::new()).with_agent(label("agent"), chosen);

    let agent = port
        .agent(&fields(), instance_id(), &label("agent"))
        .await
        .unwrap();

    assert_eq!(agent, chosen);
}

#[tokio::test]
async fn given_the_simple_thread_port_and_tools_port_then_they_give_what_the_test_sets() {
    let thread = thread_ref();
    let threads = FixedThreadPort::new(&Record::new(), thread);
    let tools = FixedToolsPort::new(&Record::new(), Vec::new());

    assert_eq!(
        ThreadPort::<Doc>::thread(&threads, &fields())
            .await
            .unwrap(),
        thread
    );
    assert!(
        ToolsPort::<Doc>::tools(&tools, &fields(), instance_id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn given_a_conversation_set_by_the_test_when_the_in_memory_thread_access_reads_then_it_gives_that_conversation_with_its_identity()
 {
    let identity = conversation_id();
    let mut conversation = Conversation::new(identity);
    conversation
        .push_turn(br_llm_messages::Turn::from(
            br_llm_messages::InputTurn::new(turn_id(), vec![human("hello").into_entry()]).unwrap(),
        ))
        .unwrap();
    let world = World::with_conversation(conversation.clone());

    let read = world.access.read(&world.thread).await.unwrap();

    assert_eq!(read, conversation);
    assert_eq!(read.id(), identity);
}

#[tokio::test]
async fn given_no_conversation_set_by_the_test_when_the_in_memory_thread_access_reads_then_it_gives_an_empty_conversation_with_the_identity_the_test_set()
 {
    let identity = conversation_id();
    let record = Record::new();
    let thread = thread_ref();
    let access = MemoryThreadAccess::new(&record, thread, Conversation::new(identity));

    let read = access.read(&thread).await.unwrap();

    assert!(read.turns().is_empty());
    assert_eq!(read.id(), identity);
}

#[tokio::test]
async fn given_a_thread_reference_that_names_another_thread_when_the_in_memory_thread_access_is_called_then_it_is_a_fault()
 {
    let world = World::new();

    let error = world.access.read(&thread_ref()).await.unwrap_err();

    assert!(!error.is_refusal());
}

#[tokio::test]
async fn given_calls_on_every_in_memory_port_then_one_shared_record_keeps_them_in_the_order_received()
 {
    let world = World::new();
    let threads = FixedThreadPort::new(&world.record, world.thread);
    let agents = MemoryAgentPort::<Doc>::new(&world.record);
    let tools = FixedToolsPort::new(&world.record, Vec::new());
    let workspace = MemoryWorkspace::new(&world.record, Vec::new());

    ThreadPort::<Doc>::thread(&threads, &fields())
        .await
        .unwrap();
    agents
        .agent(&fields(), instance_id(), &label("agent"))
        .await
        .unwrap();
    ToolsPort::<Doc>::tools(&tools, &fields(), instance_id())
        .await
        .unwrap();
    FileSystemMode::Always
        .start(&workspace, &fields())
        .await
        .unwrap();
    world.access.read(&world.thread).await.unwrap();
    world
        .inbox
        .items(&world.context(), None, limit(1))
        .await
        .unwrap();

    let kinds: Vec<&str> = world
        .record
        .calls()
        .iter()
        .map(|call| match call {
            Call::ThreadPort => "thread",
            Call::AgentPort { .. } => "agent",
            Call::ToolsPort { .. } => "tools",
            Call::WorkspaceLoad => "load",
            Call::Read { .. } => "read",
            Call::InboxItems { .. } => "items",
            _ => "other",
        })
        .collect();
    assert_eq!(&kinds[..5], ["thread", "agent", "tools", "load", "read"]);
}

#[tokio::test]
async fn given_deltas_and_fragments_sent_to_the_streams_when_the_test_reads_the_record_then_they_are_in_it_in_order()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, call) = world.open_turn_with_call(&mut session).await;
    let response = entry_id();
    let result = entry_id();

    let mut result_stream = session.open_result(&turn, result).await.unwrap();
    result_stream.send("one".to_owned()).await;
    result_stream.send("two".to_owned()).await;
    let _ = session.open_response(&turn, response).await.unwrap();
    session
        .complete_result(&turn, tool_results(result, call, "lookup", "done"))
        .await
        .unwrap();

    let fragments: Vec<String> = world
        .record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::ResultFragment { fragment, .. } => Some(fragment),
            _ => None,
        })
        .collect();
    assert_eq!(fragments, ["one", "two"]);
}

#[tokio::test]
async fn given_an_item_delivered_to_the_in_memory_inbox_then_the_in_memory_thread_access_does_not_know_it()
 {
    let world = World::new();
    world.inbox.deliver_entry(world.agent, human("hello"));

    let conversation = world.access.read(&world.thread).await.unwrap();

    assert!(conversation.turns().is_empty());
}

#[tokio::test]
async fn given_an_addition_to_the_in_memory_thread_access_then_the_in_memory_inbox_is_not_changed()
{
    let world = World::new();
    let item = human("hello");
    world.inbox.deliver_entry(world.agent, item.clone());
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    session
        .add_entry(
            &turn,
            TurnEntry::try_from(item.clone().into_entry()).unwrap(),
        )
        .await
        .unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert_eq!(batch.items.len(), 1);
    assert!(!world.inbox.is_read(world.agent, item.id()));
}
