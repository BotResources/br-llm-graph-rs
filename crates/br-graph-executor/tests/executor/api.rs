use std::sync::Arc;

use br_graph_contract::CancelToken;
use br_graph_executor::{
    Checkpoint, Observation, Outcome, ResumeError, RunConfig, RunFailure, StartError, resume, start,
};
use std::time::Duration;

use crate::graphs::*;
use crate::host::handwritten_checkpoint;
use crate::kv::*;

fn is_send<T: Send>(_: &T) {}

fn is_send_sync<T: Send + Sync>() {}

#[test]
fn given_the_public_types_when_checked_for_threads_then_runs_and_results_are_send() {
    let rig = single("a");
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let started = start(Arc::clone(&rig.graph), state(&[]), vec!["a".into()], config);
    is_send(&started);
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let checkpoint = handwritten_checkpoint(&rig, r#"{"values":{},"pad":""}"#, &["a"]);
    let resumed = resume(
        Arc::clone(&rig.graph),
        checkpoint,
        std::num::NonZeroUsize::MIN,
        config,
    );
    is_send(&resumed);
    is_send_sync::<Outcome<Kv>>();
    is_send_sync::<Checkpoint>();
    is_send_sync::<StartError>();
    is_send_sync::<ResumeError>();
    is_send_sync::<RunFailure>();
    is_send_sync::<Observation>();
}
