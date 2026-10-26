use br_graph_contract::Fields;

use super::Run;
use super::guard::AbortOnDrop;
use crate::panic_text::panic_reason;
use crate::{Checkpoint, RunFailure};

impl<F: Fields> Run<F> {
    fn non_idle_names(&self) -> Vec<String> {
        self.slots
            .iter()
            .filter(|(_, slot)| !slot.is_idle())
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub(super) fn outcome_checkpoint(&mut self) -> Checkpoint {
        let nodes = self.non_idle_names();
        match Checkpoint::build(&self.state, &self.structure, nodes.clone()) {
            Ok(checkpoint) => checkpoint,
            Err(reason) => {
                self.failures
                    .push(RunFailure::CheckpointNotBuilt { reason });
                Checkpoint::unreadable(&self.structure, nodes)
            }
        }
    }

    pub(super) async fn offer_checkpoint_to_sink(&mut self) {
        let Some(sink) = self.sink.clone() else {
            return;
        };
        let checkpoint =
            match Checkpoint::build(&self.state, &self.structure, self.non_idle_names()) {
                Ok(checkpoint) => checkpoint,
                Err(reason) => {
                    self.fail(RunFailure::CheckpointNotBuilt { reason });
                    return;
                }
            };
        let mut call = AbortOnDrop(tokio::spawn(async move { sink.accept(&checkpoint).await }));
        let reason = match (&mut call.0).await {
            Ok(Ok(())) => return,
            Ok(Err(refusal)) => refusal.reason().to_owned(),
            Err(error) if error.is_panic() => panic_reason(&*error.into_panic()),
            Err(_) => "the sink call was cancelled".to_owned(),
        };
        self.fail(RunFailure::SinkRefused { reason });
    }
}
