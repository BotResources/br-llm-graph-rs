use br_agentic_contract::{AgentLabel, RunContext, ToolSet};
use br_graph_contract::{Fields, InstanceId};
use br_llm_messages::{ClosureReason, PlainText};

use crate::config::Common;
use crate::fault::Fault;
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
        if specs.len() != agents.len() {
            return Err(Fault::Value(
                "the node has not one agent for each seat".to_owned(),
            ));
        }
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
            let shape = match (&common.ports.output_schema, spec.shape) {
                (Some(port), true) => port.shape(snapshot, &label).await?,
                _ => None,
            };
            let author = spec.author.resolve(snapshot, &label).await?;
            seats.push(Seat {
                label,
                agent,
                author,
                system,
                shape,
            });
        }
        let seats = Seats(seats);
        let main = seats.main()?;
        self.author = Some(main.author.clone());
        self.system = main.system.clone();
        self.shape = main.shape.clone();
        self.known = vec![(main.agent, main.author.clone())];
        let run_context = RunContext::new(
            instance,
            self.context.thread,
            main.agent,
            main.author.clone(),
            self.session.conversation().id(),
        );
        let tools = common.ports.tools.tools(snapshot, &run_context).await?;
        self.tools = ToolSet::new(tools, Vec::new())?;
        self.drain_at_start().await?;
        Ok(seats)
    }
}
