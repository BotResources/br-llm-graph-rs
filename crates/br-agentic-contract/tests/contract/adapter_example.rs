use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use br_agentic_contract::{
    AgentLabel, AgentPort, Cursor, Fields, IncomingBatch, IncomingContext, IncomingPort,
    InstanceId, PortError, PortFuture, RunContext, ThreadPort, ThreadRef, Tool, ToolsPort,
};
use br_llm_messages::{AgentId, EntryId, Uuid};

use crate::common::*;

struct ExampleThreadPort {
    thread: ThreadRef,
}

impl<F: Fields> ThreadPort<F> for ExampleThreadPort {
    fn thread<'a>(&'a self, _snapshot: &'a F) -> PortFuture<'a, Result<ThreadRef, PortError>> {
        Box::pin(async move { Ok(self.thread) })
    }
}

struct ExampleAgentPort {
    saved: Mutex<HashMap<String, AgentId>>,
}

impl<F: Fields> AgentPort<F> for ExampleAgentPort {
    fn agent<'a>(
        &'a self,
        _snapshot: &'a F,
        _instance: InstanceId,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<AgentId, PortError>> {
        Box::pin(async move {
            let mut saved = self
                .saved
                .lock()
                .map_err(|_| PortError::fault("the saved agents are poisoned"))?;
            if let Some(agent) = saved.get(label.as_str()) {
                return Ok(*agent);
            }
            let agent = AgentId::from_uuid(Uuid::now_v7()).map_err(PortError::fault)?;
            saved.insert(label.as_str().to_owned(), agent);
            Ok(agent)
        })
    }
}

struct ExampleToolsPort;

impl<F: Fields> ToolsPort<F> for ExampleToolsPort {
    fn tools<'a>(
        &'a self,
        _snapshot: &'a F,
        _context: &'a RunContext,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct ExampleIncoming;

impl IncomingPort for ExampleIncoming {
    fn items<'a>(
        &'a self,
        _context: &'a IncomingContext,
        _after: Option<&'a Cursor>,
        _limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        Box::pin(async {
            Ok(IncomingBatch {
                items: Vec::new(),
                next: None,
            })
        })
    }

    fn mark_read<'a>(
        &'a self,
        _context: &'a IncomingContext,
        _item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn given_an_adapter_written_with_the_imports_of_the_contract_alone_then_it_serves_as_the_ports_behind_trait_objects()
 {
    let thread = thread_ref();
    let threads: Box<dyn ThreadPort<Doc>> = Box::new(ExampleThreadPort { thread });
    let agents: Box<dyn AgentPort<Doc>> = Box::new(ExampleAgentPort {
        saved: Mutex::new(HashMap::new()),
    });
    let tools: Box<dyn ToolsPort<Doc>> = Box::new(ExampleToolsPort);
    let incoming: Box<dyn IncomingPort> = Box::new(ExampleIncoming);
    let label = AgentLabel::new("agent").unwrap();

    assert_eq!(threads.thread(&fields()).await.unwrap(), thread);
    let first = agents
        .agent(&fields(), instance_id(), &label)
        .await
        .unwrap();
    let again = agents
        .agent(&fields(), instance_id(), &label)
        .await
        .unwrap();
    assert_eq!(first, again);
    assert!(
        tools
            .tools(&fields(), &run_context(thread, first))
            .await
            .unwrap()
            .is_empty()
    );
    let context = IncomingContext {
        instance: instance_id(),
        thread,
        agent: first,
    };
    let batch = incoming.items(&context, None, limit(5)).await.unwrap();
    assert!(batch.items.is_empty() && batch.next.is_none());
    incoming.mark_read(&context, entry_id()).await.unwrap();
}
