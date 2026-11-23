use br_agentic_contract::memory::{Call, FixedToolsPort, Record};
use br_agentic_contract::{Fields, Reason};
use br_agentic_harness_contract::{RunContext, ToolsPort};
use br_llm_messages::Author;
use serde::{Deserialize, Serialize};

use crate::common::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Doc {
    n: i64,
}

impl Fields for Doc {
    type Update = i64;

    fn empty_update() -> i64 {
        0
    }

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, update: i64) -> Result<(), Reason> {
        self.n += update;
        Ok(())
    }
}

#[tokio::test]
async fn given_the_tools_port_of_the_node_library_when_the_harness_asks_it_then_the_context_carries_instance_thread_agent_author_and_conversation_id()
-> Res {
    let record = Record::new();
    let port = FixedToolsPort::new(&record, Vec::new());
    let context = RunContext::new(
        instance_id()?,
        thread_ref()?,
        agent_id()?,
        Author::new("main")?,
        conversation_id()?,
    );

    ToolsPort::<Doc>::tools(&port, &Doc { n: 1 }, &context).await?;

    let seen = record.calls().into_iter().find_map(|call| match call {
        Call::ToolsPort { context } => Some(context),
        _ => None,
    });
    assert_eq!(seen, Some(context));
    Ok(())
}
