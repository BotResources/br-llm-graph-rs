use std::sync::Arc;

use br_agentic_contract::{IncomingContext, PortError, RunContext, Tool};
use br_agentic_harness_contract::{MailPort, SandboxPort};

use crate::get_mail_files::GetMailFiles;
use crate::mail_spec::{get_mail_files_spec, send_mail_spec};
use crate::send_mail::SendMail;
use crate::shared::Shared;

pub(crate) fn mail_tools(
    mail: &Arc<dyn MailPort>,
    sandbox: &Option<Arc<dyn SandboxPort>>,
    shared: &Arc<Shared>,
    run: &RunContext,
) -> Result<Vec<Tool>, PortError> {
    let context = IncomingContext {
        instance: run.instance,
        thread: run.thread,
        agent: run.agent,
    };
    let send = Tool::new(
        send_mail_spec()?,
        Arc::new(SendMail {
            mail: Arc::clone(mail),
            context,
            author: run.author.clone(),
            source: run.conversation,
        }),
    );
    let files = Tool::new(
        get_mail_files_spec()?,
        Arc::new(GetMailFiles {
            mail: Arc::clone(mail),
            sandbox: sandbox.clone(),
            context,
            shared: Arc::clone(shared),
        }),
    );
    Ok(vec![send, files])
}
