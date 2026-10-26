use std::sync::Arc;

use br_graph_contract::Fields;

use super::Run;
use super::guard::AbortGuard;
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

    pub(super) fn outcome_checkpoint(&self) -> (Checkpoint, Option<RunFailure>) {
        let nodes = self.non_idle_names();
        match Checkpoint::build(&self.state, &self.structure, nodes.clone()) {
            Ok(checkpoint) => (checkpoint, None),
            Err(reason) => (
                Checkpoint::unreadable(&self.structure, nodes),
                Some(RunFailure::CheckpointNotBuilt { reason }),
            ),
        }
    }

    pub(super) async fn offer_checkpoint_to_sink(&mut self) {
        let Some(sink) = self.sink.clone() else {
            return;
        };
        let checkpoint =
            match Checkpoint::build(&self.state, &self.structure, self.non_idle_names()) {
                Ok(checkpoint) => Arc::new(checkpoint),
                Err(reason) => {
                    self.fail(RunFailure::CheckpointNotBuilt { reason });
                    return;
                }
            };
        let offered = Arc::clone(&checkpoint);
        let call = tokio::spawn(async move { sink.accept(&offered).await });
        let _abort_with_this_step = AbortGuard::new(call.abort_handle());
        let reason = match call.await {
            Ok(Ok(())) => return,
            Ok(Err(refusal)) => refusal.reason().to_owned(),
            Err(error) if error.is_panic() => panic_reason(&*error.into_panic()),
            Err(_) => "the sink call was cancelled".to_owned(),
        };
        let checkpoint = Arc::try_unwrap(checkpoint).unwrap_or_else(|shared| (*shared).clone());
        self.fail(RunFailure::SinkRefused { reason, checkpoint });
    }
}
