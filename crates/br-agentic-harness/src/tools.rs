use std::sync::Arc;

use br_agentic_contract::{Fields, PortError, PortFuture, RunContext, Tool, ToolSet, ToolsPort};
use br_agentic_harness_contract::{MailPort, SandboxPort};

use crate::mail_tools::mail_tools;
use crate::shared::Shared;

pub struct ToolsAdapter<F: Fields> {
    inner: Arc<dyn ToolsPort<F>>,
    mail: Option<Arc<dyn MailPort>>,
    sandbox: Option<Arc<dyn SandboxPort>>,
    shared: Arc<Shared>,
}

impl<F: Fields> ToolsAdapter<F> {
    pub(crate) fn new(
        inner: Arc<dyn ToolsPort<F>>,
        mail: Option<Arc<dyn MailPort>>,
        sandbox: Option<Arc<dyn SandboxPort>>,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            inner,
            mail,
            sandbox,
            shared,
        }
    }
}

impl<F: Fields> ToolsPort<F> for ToolsAdapter<F> {
    fn tools<'a>(
        &'a self,
        snapshot: &'a F,
        context: &'a RunContext,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        Box::pin(async move {
            let runner = self.inner.tools(snapshot, context).await?;
            let harness = match &self.mail {
                Some(mail) => mail_tools(mail, &self.sandbox, &self.shared, context)?,
                None => Vec::new(),
            };
            let set = ToolSet::new(runner, harness)
                .map_err(|clash| PortError::refused(clash.to_string()))?;
            Ok(set.tools().to_vec())
        })
    }
}
