use std::time::Duration;

use br_agentic_contract::{AiTurnRef, ResultStreamBox, ToolContent, ToolFault};
use br_llm_messages::{
    BlockId, EntryId, EntryView, ModelResponse, RuntimeToolCall, RuntimeToolResults, ToolName,
    TurnId,
};
use tokio::task::JoinSet;
use tokio::time::error::Elapsed;

use crate::fault::{Fault, Stop};
use crate::ids::{block_id, entry_id};
use crate::mail_tools::SEND_MAIL;
use crate::worker::Worker;

struct Finished {
    entry: EntryId,
    call: BlockId,
    name: ToolName,
    timeout: Duration,
    ended: Result<Result<ToolContent, ToolFault>, Elapsed>,
}

struct Immediate {
    entry: EntryId,
    call: BlockId,
    name: ToolName,
    content: ToolContent,
}

fn timeout_text(name: &ToolName, timeout: Duration) -> String {
    let mut text = format!(
        "the call to the tool {} exceeded its timeout of {timeout:?}",
        name.as_str()
    );
    if name.as_str() == SEND_MAIL {
        text.push_str("; it is not known whether the mail was sent");
    }
    text
}

impl Worker<'_> {
    fn refresh_gate(&self) {
        let mut ids = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ids.clear();
        for placed in self.session.conversation().entries() {
            let incoming = match placed.entry() {
                EntryView::Incoming(_) => true,
                EntryView::Turn(entry) => entry.is_incoming(),
                _ => false,
            };
            if incoming {
                ids.insert(placed.entry().id());
            }
        }
    }

    pub async fn tool_round(&mut self, turn: TurnId, response: &ModelResponse) -> Result<(), Stop> {
        self.refresh_gate();
        let target = self.turn_ref(turn)?;
        let mut opened = Vec::new();
        for call in response.runtime_calls() {
            let entry = entry_id()?;
            let stream = self.session.open_result(&target, entry).await?;
            opened.push((call, entry, stream));
        }
        let mut running = JoinSet::new();
        let mut immediate = Vec::new();
        for (call, entry, stream) in opened {
            if let Some(answer) = self.start_call(call, entry, stream, &mut running)? {
                immediate.push(answer);
            }
        }
        for answer in immediate {
            self.complete_call(
                &target,
                answer.entry,
                answer.call,
                answer.name,
                answer.content,
            )
            .await?;
        }
        while let Some(joined) = running.join_next().await {
            let finished = joined.map_err(|error| Fault::Panic(error.to_string()))?;
            if self.cancel.is_triggered() {
                return Err(Stop::Cancelled);
            }
            let content = match finished.ended {
                Err(_) => ToolContent::error_text(timeout_text(&finished.name, finished.timeout))
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

    fn start_call(
        &mut self,
        call: &RuntimeToolCall,
        entry: EntryId,
        stream: ResultStreamBox,
        running: &mut JoinSet<Finished>,
    ) -> Result<Option<Immediate>, Fault> {
        let name = call.name().clone();
        let Some(tool) = self.tools.find(name.as_str()).cloned() else {
            let content =
                ToolContent::error_text(format!("the node holds no tool named {}", name.as_str()))
                    .map_err(Fault::value)?;
            drop(stream);
            return Ok(Some(Immediate {
                entry,
                call: call.id(),
                name,
                content,
            }));
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
        Ok(None)
    }

    async fn complete_call(
        &mut self,
        target: &AiTurnRef,
        entry: EntryId,
        call: BlockId,
        name: ToolName,
        content: ToolContent,
    ) -> Result<(), Fault> {
        let mut ids = (0..content.content().len())
            .map(|_| block_id())
            .collect::<Result<Vec<_>, _>>()?;
        let mut exhausted = false;
        let result = content
            .into_result(block_id()?, call, name, || {
                ids.pop().unwrap_or_else(|| {
                    exhausted = true;
                    call
                })
            })
            .map_err(Fault::value)?;
        if exhausted {
            return Err(Fault::Value(
                "a tool result needed more block identities than it holds blocks".to_owned(),
            ));
        }
        let results = RuntimeToolResults::new(entry, vec![result]).map_err(Fault::value)?;
        self.session.complete_result(target, results).await?;
        Ok(())
    }
}
