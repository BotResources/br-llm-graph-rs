use br_agentic_contract::{Fields, InstanceId, PortError, PortFuture};
use br_agentic_harness_contract::{
    AlreadySaved, FreshReason, OpenRequest, ProcessId, RunEnd, RunLifecycle, SandboxPort, Seed,
    StartKind, TurnRef,
};

use crate::harness::Harness;
use crate::runs::{Closing, Committed, Run};

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

async fn shut(sandbox: &dyn SandboxPort, run: &Run, end: RunEnd) -> Option<PortError> {
    let mut first = None;
    for process in &run.background {
        if let Err(failure) = sandbox.kill(&run.boxed, process).await {
            first.get_or_insert(failure);
        }
    }
    if end == RunEnd::Completed
        && let Err(failure) = sandbox.save(&run.boxed).await
        && !AlreadySaved::is(&failure)
    {
        first.get_or_insert(failure);
    }
    if let Err(failure) = sandbox.release(&run.boxed).await {
        first.get_or_insert(failure);
    }
    first
}

impl<F: Fields> RunLifecycle for Harness<F> {
    fn open<'a>(
        &'a self,
        instance: InstanceId,
        turn: TurnRef,
        seed: Seed,
    ) -> PortFuture<'a, Result<StartKind, PortError>> {
        Box::pin(async move {
            let Some(sandbox) = &self.sandbox else {
                return Ok(StartKind::Fresh(FreshReason::NoSaveEver));
            };
            let Some(reservation) = self.shared.runs.reserve(instance) else {
                return Err(PortError::refused("the instance is open already"));
            };
            let request = OpenRequest::new(instance, self.web.clone(), turn, seed);
            let opened = sandbox.open(&request).await?;
            match reservation.commit(opened.box_id) {
                Committed::Kept => Ok(opened.start),
                Committed::ClosedMeanwhile(run, end) => {
                    Err(shut(sandbox.as_ref(), &run, end).await.unwrap_or_else(|| {
                        PortError::refused("the instance was closed while it opened")
                    }))
                }
            }
        })
    }

    fn close<'a>(
        &'a self,
        instance: InstanceId,
        end: RunEnd,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.shared.mails.release(instance);
            let Some(sandbox) = &self.sandbox else {
                return Ok(());
            };
            match self.shared.runs.close(instance, end) {
                Closing::Open(run) => shut(sandbox.as_ref(), &run, end).await.map_or(Ok(()), Err),
                Closing::Opening | Closing::Nothing => Ok(()),
            }
        })
    }
}
