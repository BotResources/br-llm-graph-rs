use std::sync::Arc;

use br_agentic_contract::{Agent, AgentLabel, ToolSet, save_files};
use br_graph_contract::{Fields, InstanceId};
use br_llm_messages::Author;

use crate::config::Common;
use crate::fault::Fault;
use crate::mail_tools::mail_tools;
use crate::ports::Outcome;
use crate::sandbox::FileSystem;
use crate::worker::Worker;

pub(crate) fn agent_label(text: &str) -> Result<AgentLabel, Fault> {
    AgentLabel::new(text).map_err(Fault::value)
}

pub(crate) fn author_of(
    build: Option<&Author>,
    agent: &Agent,
    label: &AgentLabel,
) -> Result<Author, Fault> {
    build
        .cloned()
        .or_else(|| agent.author.clone())
        .ok_or_else(|| Fault::NoAuthor {
            label: label.to_string(),
        })
}

impl Worker<'_> {
    pub(crate) async fn begin<F: Fields>(
        &mut self,
        common: &Common<F>,
        snapshot: &F,
        instance: InstanceId,
        label: &AgentLabel,
    ) -> Result<(), Fault> {
        self.interrupt_open_turn().await?;
        self.system = Some(common.ports.prompt.prompt(snapshot, label).await?);
        let tools = common.ports.tools.tools(snapshot, instance).await?;
        let mail = match &common.ports.inbox {
            Some(inbox) => mail_tools(
                Arc::clone(inbox),
                Arc::clone(&common.ports.access),
                self.context,
            )?,
            None => Vec::new(),
        };
        self.tools = ToolSet::new(tools, mail)?;
        let loaded = common
            .settings
            .file_system
            .start(&*common.ports.workspace, snapshot)
            .await?;
        self.files = loaded
            .map(FileSystem::from_files)
            .transpose()
            .map_err(|error| Fault::Sandbox(error.to_string()))?;
        self.drain().await?;
        Ok(())
    }
}

impl Worker<'_> {
    pub(crate) async fn end<F: Fields>(
        &mut self,
        common: &Common<F>,
        snapshot: &F,
        outcome: Outcome,
    ) -> Result<F::Update, Fault> {
        let update = common.ports.outcome.update(snapshot, &outcome).await?;
        let files = self.files.take().map(FileSystem::into_files);
        save_files(&*common.ports.workspace, snapshot, files).await?;
        Ok(update)
    }
}
