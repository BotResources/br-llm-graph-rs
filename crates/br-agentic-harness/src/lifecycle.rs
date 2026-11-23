use br_agentic_contract::{Fields, InstanceId, PortError, PortFuture};
use br_agentic_harness_contract::{ProcessId, RunEnd, RunLifecycle};

use crate::harness::Harness;

impl<F: Fields> Harness<F> {
    pub fn track_background(
        &self,
        instance: InstanceId,
        process: ProcessId,
    ) -> Result<(), PortError> {
        if self.shared.runs.track(instance, process) {
            Ok(())
        } else {
            Err(PortError::refused("the instance has no open box"))
        }
    }
}

impl<F: Fields> RunLifecycle for Harness<F> {
    fn open<'a>(&'a self, instance: InstanceId) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            let Some(sandbox) = &self.sandbox else {
                return Ok(());
            };
            if self.shared.runs.boxed(instance).is_some() {
                return Err(PortError::refused("the instance is open already"));
            }
            let boxed = sandbox.open(instance, self.web.as_ref()).await?;
            self.shared.runs.insert(instance, boxed);
            Ok(())
        })
    }

    fn close<'a>(
        &'a self,
        instance: InstanceId,
        _end: RunEnd,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            let Some(sandbox) = &self.sandbox else {
                return Ok(());
            };
            let Some(run) = self.shared.runs.take(instance) else {
                return Ok(());
            };
            let mut first = None;
            for process in &run.background {
                if let Err(failure) = sandbox.kill(&run.boxed, process).await {
                    first.get_or_insert(failure);
                }
            }
            if let Err(failure) = sandbox.save(&run.boxed).await {
                first.get_or_insert(failure);
            }
            if let Err(failure) = sandbox.release(&run.boxed).await {
                first.get_or_insert(failure);
            }
            first.map_or(Ok(()), Err)
        })
    }
}
