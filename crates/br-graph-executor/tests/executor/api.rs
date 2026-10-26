use std::sync::Arc;

use br_graph_contract::CancelToken;
use br_graph_executor::{
    Checkpoint, Observation, Outcome, ResumeError, RunConfig, RunFailure, StartError, resume, start,
};
use std::time::Duration;

use crate::graphs::*;
use crate::kv::*;

fn is_send<T: Send>(_: &T) {}

fn is_send_sync<T: Send + Sync>() {}

#[test]
fn the_futures_of_a_run_and_its_results_can_be_sent_between_threads() {
    let rig = single("a");
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let started = start(Arc::clone(&rig.graph), state(&[]), vec!["a".into()], config);
    is_send(&started);
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let checkpoint: Option<Checkpoint> = None;
    if let Some(checkpoint) = checkpoint {
        let resumed = resume(
            Arc::clone(&rig.graph),
            checkpoint,
            std::num::NonZeroUsize::MIN,
            config,
        );
        is_send(&resumed);
    }
    is_send_sync::<Outcome<Kv>>();
    is_send_sync::<Checkpoint>();
    is_send_sync::<StartError>();
    is_send_sync::<ResumeError>();
    is_send_sync::<RunFailure>();
    is_send_sync::<Observation>();
}
