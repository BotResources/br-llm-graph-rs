use br_agentic_contract::memory::{
    FixedThreadPort, FixedToolsPort, MemoryAgentPort, MemoryInbox, MemoryThreadAccess,
    MemoryWorkspace, Record,
};
use br_agentic_contract::{
    AgentPort, InboxPort, ThreadAccess, ThreadPort, ToolCode, ToolsPort, WorkspacePort,
};
use br_llm_messages::Conversation;

use crate::common::*;

fn is_send_sync<T: Send + Sync + ?Sized>() {}

#[test]
fn given_every_port_then_a_trait_object_of_it_is_send_and_sync() {
    is_send_sync::<dyn ThreadPort<Doc>>();
    is_send_sync::<dyn AgentPort<Doc>>();
    is_send_sync::<dyn ToolsPort<Doc>>();
    is_send_sync::<dyn WorkspacePort<Doc>>();
    is_send_sync::<dyn ThreadAccess>();
    is_send_sync::<dyn InboxPort>();
    is_send_sync::<dyn ToolCode>();
}

#[test]
fn given_the_in_memory_ports_then_each_one_fits_in_a_box_of_the_trait_object_of_its_port() {
    let record = Record::new();
    let thread = thread_ref();
    let _: Box<dyn ThreadPort<Doc>> = Box::new(FixedThreadPort::new(&record, thread));
    let _: Box<dyn AgentPort<Doc>> = Box::new(MemoryAgentPort::<Doc>::new(&record));
    let _: Box<dyn ToolsPort<Doc>> = Box::new(FixedToolsPort::new(&record, Vec::new()));
    let _: Box<dyn WorkspacePort<Doc>> = Box::new(MemoryWorkspace::new(&record, Vec::new()));
    let _: Box<dyn ThreadAccess> = Box::new(MemoryThreadAccess::new(
        &record,
        thread,
        Conversation::new(conversation_id()),
    ));
    let _: Box<dyn InboxPort> = Box::new(MemoryInbox::new(&record));
}

#[test]
fn given_the_ports_then_several_instances_can_share_one_port_behind_an_arc() {
    let record = Record::new();
    let _: std::sync::Arc<dyn InboxPort> = std::sync::Arc::new(MemoryInbox::new(&record));
    let _: std::sync::Arc<dyn ThreadAccess> = std::sync::Arc::new(MemoryThreadAccess::new(
        &record,
        thread_ref(),
        Conversation::new(conversation_id()),
    ));
}
