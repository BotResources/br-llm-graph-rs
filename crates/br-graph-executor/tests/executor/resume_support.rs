use br_graph_executor::{Checkpoint, ResumeError, resume};
use serde_json::Value;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

pub async fn checkpoint_of(rig: &Rig) -> Checkpoint {
    let host = begin(rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("stop here");
    failed(host.outcome().await).1
}

pub fn forged(checkpoint: &Checkpoint, edit: impl FnOnce(&mut Value)) -> Checkpoint {
    let mut value: Value =
        serde_json::from_slice(&serde_json::to_vec(checkpoint).unwrap()).unwrap();
    edit(&mut value);
    Checkpoint::from_json_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

pub async fn refusal(rig: &Rig, checkpoint: Checkpoint) -> ResumeError {
    let (config, ..) = Setup::new().config();
    within(resume(rig.graph.clone(), checkpoint, BIG, config))
        .await
        .unwrap_err()
}

pub fn base() -> Rig {
    chain("1", &["a", "b"])
}
