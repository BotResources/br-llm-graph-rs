use br_agentic_contract::memory::{
    FixedThreadPort, FixedToolsPort, MemoryAgentPort, MemoryIncoming, MemoryThreadAccess, Record,
};
use br_agentic_contract::{
    AgentPort, AuthorPort, CriticRequestPort, GeneratorCriticOutcomePort, IncomingPort,
    OutputSchemaPort, ProgrammaticCriticPort, PromptPort, ReactOutcomePort, ThreadAccess,
    ThreadPort, ToolCode, ToolsPort,
};
use br_llm_messages::Conversation;

use crate::common::*;

fn is_send_sync<T: Send + Sync + ?Sized>() {}

#[test]
fn given_every_port_then_a_trait_object_of_it_is_send_and_sync() {
    is_send_sync::<dyn ThreadPort<Doc>>();
    is_send_sync::<dyn AgentPort<Doc>>();
    is_send_sync::<dyn ToolsPort<Doc>>();
    is_send_sync::<dyn ThreadAccess>();
    is_send_sync::<dyn IncomingPort>();
    is_send_sync::<dyn ToolCode>();
    is_send_sync::<dyn PromptPort<Doc>>();
    is_send_sync::<dyn OutputSchemaPort<Doc>>();
    is_send_sync::<dyn AuthorPort<Doc>>();
    is_send_sync::<dyn ReactOutcomePort<Doc>>();
    is_send_sync::<dyn GeneratorCriticOutcomePort<Doc>>();
    is_send_sync::<dyn CriticRequestPort<Doc>>();
    is_send_sync::<dyn ProgrammaticCriticPort<Doc>>();
}

#[test]
fn given_the_in_memory_ports_then_each_one_fits_in_a_box_of_the_trait_object_of_its_port() {
    let record = Record::new();
    let thread = thread_ref();
    let _: Box<dyn ThreadPort<Doc>> = Box::new(FixedThreadPort::new(&record, thread));
    let _: Box<dyn AgentPort<Doc>> = Box::new(MemoryAgentPort::<Doc>::new(&record));
    let _: Box<dyn ToolsPort<Doc>> = Box::new(FixedToolsPort::new(&record, Vec::new()));
    let _: Box<dyn ThreadAccess> = Box::new(MemoryThreadAccess::new(
        &record,
        thread,
        Conversation::new(conversation_id()),
    ));
    let _: Box<dyn IncomingPort> = Box::new(MemoryIncoming::new(&record));
}

#[test]
fn given_the_ports_then_several_instances_can_share_one_port_behind_an_arc() {
    let record = Record::new();
    let _: std::sync::Arc<dyn IncomingPort> = std::sync::Arc::new(MemoryIncoming::new(&record));
    let _: std::sync::Arc<dyn ThreadAccess> = std::sync::Arc::new(MemoryThreadAccess::new(
        &record,
        thread_ref(),
        Conversation::new(conversation_id()),
    ));
}

fn is_send<T: Send>(_: T) {}

#[test]
fn given_the_calls_of_the_ports_then_their_futures_are_send_so_a_runner_can_spawn_an_instance() {
    let world = World::new();
    let context = world.context();
    is_send(async {
        world.access.read(&world.thread).await.ok();
        world.incoming.items(&context, None, limit(1)).await.ok();
        world.incoming.mark_read(&context, entry_id()).await.ok();
    });
}
