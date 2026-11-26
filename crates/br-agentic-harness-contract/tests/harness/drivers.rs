use std::collections::HashSet;

use br_agentic_harness_contract::{
    BoxId, FilePath, HarnessHistory, IncomingContext, MailId, MailPort, PortError, ProcessId,
    RunEnd, SandboxPort, StoredAnswer,
};
use br_llm_messages::EntryId;

pub async fn mark_when_stored(
    history: &dyn HarnessHistory,
    mail: &dyn MailPort,
    context: &IncomingContext,
    entry: EntryId,
) -> Result<bool, PortError> {
    match history.stored(&context.thread, entry).await? {
        StoredAnswer::Stored => {
            mail.mark_read(context, entry).await?;
            Ok(true)
        }
        StoredAnswer::Dropped => Ok(false),
    }
}

pub async fn close_run(
    sandbox: &dyn SandboxPort,
    boxed: &BoxId,
    background: &[ProcessId],
    end: RunEnd,
) -> Result<(), PortError> {
    for process in background {
        sandbox.kill(boxed, process).await?;
    }
    if end == RunEnd::Completed {
        sandbox.save(boxed).await?;
    }
    sandbox.release(boxed).await
}

#[derive(Default)]
pub struct Gate {
    seen: HashSet<MailId>,
}

impl Gate {
    pub fn see(&mut self, mail: MailId) {
        self.seen.insert(mail);
    }

    pub fn check(&self, mail: MailId) -> Result<(), PortError> {
        if self.seen.contains(&mail) {
            Ok(())
        } else {
            Err(PortError::refused(
                "the mail id was not seen by the history",
            ))
        }
    }
}

pub async fn copy_attachments(
    gate: &Gate,
    mail: &dyn MailPort,
    context: &IncomingContext,
    sandbox: &dyn SandboxPort,
    boxed: &BoxId,
    id: MailId,
) -> Result<usize, PortError> {
    gate.check(id)?;
    let documents = mail.documents(context, id).await?;
    for document in &documents {
        let path =
            FilePath::new(&format!("mail/{id}/{}", document.name)).map_err(PortError::fault)?;
        sandbox
            .write(boxed, &path, document.content.clone())
            .await?;
    }
    Ok(documents.len())
}
