//! The test clients and observers that are not the mock: a client that
//! serves its steps without the checks of a conforming client, and an
//! observer that keeps the stream events.

use std::collections::VecDeque;
use std::sync::Mutex;

use br_llm_clients_core::{
    BoxFuture, CallOutcome, CallReport, Client, ClientError, ClientRequest, ClientSettings,
    ReasoningLevel, StreamSink,
};
use br_llm_messages::Step;

use crate::react::test_support::settings;
use crate::value::Key;

/// A client that serves its steps as they are, without the checks a
/// conforming client makes of its replies: the client a test writes for a
/// reply the mock refuses, such as a step holding only thinking. It refuses
/// a list of messages like every client library (`check_messages`), sends
/// no stream event and keeps every request.
pub(crate) struct LaxClient {
    settings: ClientSettings,
    steps: Mutex<VecDeque<Step>>,
    requests: Mutex<Vec<ClientRequest>>,
}

impl LaxClient {
    pub(crate) fn new(steps: Vec<Step>) -> Self {
        Self {
            settings: settings(ReasoningLevel::Medium),
            steps: Mutex::new(steps.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn requests(&self) -> Vec<ClientRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Client for LaxClient {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        _sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        self.requests.lock().unwrap().push(request.clone());
        let checked = request.check_messages();
        let step = match checked {
            Ok(()) => self.steps.lock().unwrap().pop_front(),
            Err(_) => None,
        };
        Box::pin(async move {
            let report = CallReport::new(std::time::Duration::ZERO);
            if let Err(error) = checked {
                return CallOutcome::failure(report, error);
            }
            match step {
                Some(step) => {
                    let report = match step.usage() {
                        Some(usage) => report.with_usage(usage.clone()),
                        None => report,
                    };
                    CallOutcome::success(report, step).unwrap()
                }
                None => CallOutcome::failure(
                    report,
                    ClientError::ProviderRejected {
                        status: None,
                        code: None,
                        body: "the lax client has no step left".to_owned(),
                    },
                ),
            }
        })
    }

    fn settings(&self) -> &ClientSettings {
        &self.settings
    }
}

/// Keeps every stream event the observer receives, with the occurrence it
/// comes from and its conversation key.
#[derive(Default)]
pub(crate) struct StreamRecorder {
    events: Mutex<Vec<(String, String, br_llm_messages::StreamEvent)>>,
}

impl StreamRecorder {
    /// The events received, as `(occurrence, key, event)`, in order.
    pub(crate) fn events(&self) -> Vec<(String, String, br_llm_messages::StreamEvent)> {
        self.events.lock().unwrap().clone()
    }
}

impl crate::observe::Observer for StreamRecorder {
    fn stream(
        &self,
        origin: &crate::origin::Origin,
        key: &Key,
        event: &br_llm_messages::StreamEvent,
    ) {
        self.events.lock().unwrap().push((
            origin.occurrence.to_string(),
            key.as_str().to_owned(),
            event.clone(),
        ));
    }
}
