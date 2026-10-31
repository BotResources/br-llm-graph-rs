use std::sync::Arc;

use br_agentic_contract::{AgentLabel, ToolSet, save_files};
use br_graph_contract::{Fields, InstanceId};
use br_llm_messages::{ClosureReason, PlainText};

use crate::config::Common;
use crate::fault::Fault;
use crate::mail_tools::mail_tools;
use crate::sandbox::FileSystem;
use crate::seats::{Seat, SeatSpec, Seats};
use crate::worker::Worker;

pub(crate) fn agent_label(text: &str) -> Result<AgentLabel, Fault> {
    AgentLabel::new(text).map_err(Fault::value)
}

impl Worker<'_> {
    pub(crate) async fn begin<F: Fields>(
        &mut self,
        common: &Common<F>,
        snapshot: &F,
        instance: InstanceId,
        specs: &[SeatSpec<'_, F>],
        agents: Vec<(AgentLabel, br_llm_messages::AgentId)>,
    ) -> Result<Seats, Fault> {
        self.agents = agents.iter().map(|(_, agent)| *agent).collect();
        self.close_open_turns(&ClosureReason::interrupted(), true)
            .await?;
        let mut seats = Vec::new();
        for (spec, (label, agent)) in specs.iter().zip(agents) {
            let system = if spec.prompt {
                let text = common.ports.prompt.prompt(snapshot, &label).await?;
                Some(PlainText::new(text).map_err(Fault::value)?)
            } else {
                None
            };
            let author = spec.author.resolve(snapshot, &label).await?;
            seats.push(Seat {
                label,
                agent,
                author,
                system,
            });
        }
        let seats = Seats(seats);
        let main = seats.main()?;
        self.author = Some(main.author.clone());
        self.system = main.system.clone();
        self.known = vec![(main.agent, main.author.clone())];
        let tools = common.ports.tools.tools(snapshot, instance).await?;
        let mail = match &common.ports.inbox {
            Some(inbox) => mail_tools(
                Arc::clone(inbox),
                self.context,
                main.author.clone(),
                self.session.conversation().id(),
                Arc::clone(&self.gate),
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
        self.read_inbox_now().await;
        self.drain().await?;
        Ok(seats)
    }

    pub(crate) async fn finish<F: Fields>(
        &mut self,
        common: &Common<F>,
        snapshot: &F,
        update: F::Update,
    ) -> Result<F::Update, Fault> {
        let files = self.files.take().map(FileSystem::into_files);
        save_files(&*common.ports.workspace, snapshot, files).await?;
        Ok(update)
    }
}
