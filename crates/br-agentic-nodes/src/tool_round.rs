use std::time::Duration;

use br_agentic_contract::{Opened, ResultStreamBox, ToolContent, ToolFault, TurnTarget};
use br_llm_messages::{BlockId, EntryId, ModelResponse, RuntimeToolCall, ToolName, TurnId};
use tokio::task::JoinSet;
use tokio::time::error::Elapsed;

use crate::fault::{Fault, Stop};
use crate::ids::{block_id, entry_id};
use crate::worker::Worker;

struct Finished {
    entry: EntryId,
    call: BlockId,
    name: ToolName,
    timeout: Duration,
    ended: Result<Result<ToolContent, ToolFault>, Elapsed>,
}

impl Worker<'_> {
    pub async fn tool_round(&mut self, turn: TurnId, response: &ModelResponse) -> Result<(), Stop> {
        let target = TurnTarget::Ai {
            id: turn,
            author: self.author.clone(),
        };
        let mut running = JoinSet::new();
        for call in response.runtime_calls() {
            self.start_call(&target, call, &mut running).await?;
        }
        while let Some(joined) = running.join_next().await {
            let finished = joined.map_err(|error| Fault::Panic(error.to_string()))?;
            if self.cancel.is_triggered() {
                return Err(Stop::Cancelled);
            }
            let content = match finished.ended {
                Err(_) => ToolContent::error_text(
                    block_id()?,
                    format!(
                        "the call to the tool {} exceeded its timeout of {:?}",
                        finished.name.as_str(),
                        finished.timeout
                    ),
                )
                .map_err(Fault::value)?,
                Ok(Err(fault)) => return Err(Fault::Tool(fault).into()),
                Ok(Ok(content)) => content,
            };
            self.complete_call(
                &target,
                finished.entry,
                finished.call,
                finished.name,
                content,
            )
            .await?;
        }
        Ok(())
    }

    async fn start_call(
        &mut self,
        target: &TurnTarget,
        call: &RuntimeToolCall,
        running: &mut JoinSet<Finished>,
    ) -> Result<(), Fault> {
        let entry = entry_id()?;
        let opened = self
            .session
            .open_result(target.clone(), entry, call.id())
            .await?;
        let stream: ResultStreamBox = match opened {
            Opened::Held(_) => return Ok(()),
            Opened::Stream(stream) => stream,
        };
        let name = call.name().clone();
        let Some(tool) = self.tools.find(name.as_str()).cloned() else {
            let content = ToolContent::error_text(
                block_id()?,
                format!("the node holds no tool named {}", name.as_str()),
            )
            .map_err(Fault::value)?;
            drop(stream);
            return self
                .complete_call(target, entry, call.id(), name, content)
                .await;
        };
        let arguments = call.arguments().clone();
        let cancel = self.cancel.clone();
        let timeout = self.timeouts.tool_call();
        let id = call.id();
        running.spawn(async move {
            let mut stream = stream;
            let ended =
                tokio::time::timeout(timeout, tool.call(&arguments, cancel, &mut *stream)).await;
            Finished {
                entry,
                call: id,
                name,
                timeout,
                ended,
            }
        });
        Ok(())
    }

    async fn complete_call(
        &mut self,
        target: &TurnTarget,
        entry: EntryId,
        call: BlockId,
        name: ToolName,
        content: ToolContent,
    ) -> Result<(), Fault> {
        let result = content
            .into_result(block_id()?, call, name)
            .map_err(Fault::value)?;
        self.session
            .complete_result(target.clone(), entry, result)
            .await?;
        Ok(())
    }
}
