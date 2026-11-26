use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{BoxId, OpenRequest};
use std::error::Error;

use crate::common::World;
use crate::spy::Seen;

pub fn requests(world: &World) -> Vec<OpenRequest> {
    world
        .record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { request } => Some(request),
            _ => None,
        })
        .collect()
}

pub fn steps(world: &World) -> Vec<&'static str> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { .. } => Some("open"),
            HarnessCall::SandboxKill { .. } => Some("kill"),
            HarnessCall::SandboxSave { .. } => Some("save"),
            HarnessCall::SandboxRelease { .. } => Some("release"),
            _ => None,
        })
        .collect()
}

pub fn seen_steps(seen: &Seen) -> Vec<String> {
    seen.all()
        .iter()
        .filter_map(|line| line.split(' ').next())
        .filter(|word| ["open", "kill", "save", "release"].contains(word))
        .map(str::to_owned)
        .collect()
}

pub fn released_box(world: &World) -> Result<BoxId, Box<dyn Error>> {
    world
        .record
        .calls()
        .iter()
        .find_map(|call| match call {
            HarnessCall::SandboxRelease { sandbox } => Some(sandbox.clone()),
            _ => None,
        })
        .ok_or_else(|| "no box was released".into())
}
