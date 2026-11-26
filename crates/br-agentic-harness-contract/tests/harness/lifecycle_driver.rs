use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use br_agentic_contract::{InstanceId, PortError, PortFuture, WebAccess};
use br_agentic_harness_contract::memory::MemorySandbox;
use br_agentic_harness_contract::{
    BoxId, FreshReason, OpenRequest, RunEnd, RunLifecycle, SandboxPort, Seed, StartKind, TurnRef,
};

enum Run {
    Opening { close: Option<RunEnd> },
    Open(BoxId),
}

pub struct Driven<'s> {
    sandbox: Option<&'s MemorySandbox>,
    web: Option<WebAccess>,
    runs: Mutex<HashMap<InstanceId, Run>>,
}

struct OpeningGuard<'a> {
    runs: &'a Mutex<HashMap<InstanceId, Run>>,
    instance: InstanceId,
}

impl Drop for OpeningGuard<'_> {
    fn drop(&mut self) {
        let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(runs.get(&self.instance), Some(Run::Opening { .. })) {
            runs.remove(&self.instance);
        }
    }
}

impl<'s> Driven<'s> {
    pub fn with_sandbox(sandbox: &'s MemorySandbox) -> Self {
        Self {
            sandbox: Some(sandbox),
            web: None,
            runs: Mutex::default(),
        }
    }

    pub fn without_sandbox() -> Self {
        Self {
            sandbox: None,
            web: None,
            runs: Mutex::default(),
        }
    }

    pub fn with_web(mut self, web: WebAccess) -> Self {
        self.web = Some(web);
        self
    }

    pub fn box_of(&self, instance: InstanceId) -> Option<BoxId> {
        match self
            .runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&instance)
        {
            Some(Run::Open(boxed)) => Some(boxed.clone()),
            _ => None,
        }
    }

    async fn finish(sandbox: &MemorySandbox, boxed: &BoxId, end: RunEnd) -> Result<(), PortError> {
        for process in sandbox.running(boxed) {
            sandbox.kill(boxed, &process).await?;
        }
        if end == RunEnd::Completed {
            sandbox.save(boxed).await?;
        }
        sandbox.release(boxed).await
    }
}

impl RunLifecycle for Driven<'_> {
    fn open<'a>(
        &'a self,
        instance: InstanceId,
        turn: TurnRef,
        seed: Seed,
    ) -> PortFuture<'a, Result<StartKind, PortError>> {
        Box::pin(async move {
            let Some(sandbox) = self.sandbox else {
                return Ok(StartKind::Fresh(FreshReason::NoSaveEver));
            };
            self.runs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(instance, Run::Opening { close: None });
            let _guard = OpeningGuard {
                runs: &self.runs,
                instance,
            };
            let request = OpenRequest::new(instance, self.web.clone(), turn, seed);
            let opened = sandbox.open(&request).await?;
            let closing = {
                let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
                match runs.remove(&instance) {
                    Some(Run::Opening { close: Some(end) }) => Some(end),
                    _ => {
                        runs.insert(instance, Run::Open(opened.box_id.clone()));
                        None
                    }
                }
            };
            if let Some(end) = closing {
                Self::finish(sandbox, &opened.box_id, end).await?;
            }
            Ok(opened.start)
        })
    }

    fn close<'a>(
        &'a self,
        instance: InstanceId,
        end: RunEnd,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            let Some(sandbox) = self.sandbox else {
                return Ok(());
            };
            let boxed = {
                let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
                match runs.get_mut(&instance) {
                    Some(Run::Opening { close }) => {
                        *close = Some(end);
                        return Ok(());
                    }
                    Some(Run::Open(_)) => match runs.remove(&instance) {
                        Some(Run::Open(boxed)) => boxed,
                        _ => return Ok(()),
                    },
                    None => return Ok(()),
                }
            };
            Self::finish(sandbox, &boxed, end).await
        })
    }
}
