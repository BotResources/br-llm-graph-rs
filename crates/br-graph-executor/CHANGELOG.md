# Changelog

All notable changes to `br-graph-executor` are documented here. The git tag
`br-graph-executor/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

### Added

- `start` and `resume`: run a graph of `br-graph-contract` from a state and a list of nodes, or from a checkpoint.
- `RunConfig` (cancel token, grace period, optional checkpoint sink and observer), `Outcome` (`RunEnded`, `RunFailed`, `RunCancelled`) and `RunFailure`.
- `Checkpoint`: plain serializable data (state, structure, names of the nodes that are not idle), `StartError`, `ResumeError`.
- `CheckpointSink` and `SinkRefusal`; `Observer`, `Observation`, `ObservedEvent` and `OutcomeKind`.
- `RunFailure::SinkRefused { reason, checkpoint }`: the failure carries the checkpoint the sink refused (the one handed to the sink, which holds the state of its step). The checkpoint of the outcome stays the most current one and can be newer.
- `RunFailure::CheckpointNotBuilt`: the state of a run could not be serialized (error or panic) when a checkpoint was built. The checkpoint of the outcome then holds `null` as its state: a host must not resume it. The failure can be listed twice (once for a step, once for the outcome).
- A run whose failure was detected by the executor (a sink refusal, a checkpoint that could not be built) is always `RunFailed`, also when every node is idle at the end; `RunEnded` is returned only when the executor did not stop the run. Such an outcome can carry a checkpoint that names no node: resuming it is refused as `NoNodes`.
- A `Result` already queued when the grace period ends is applied, and its edge evaluated, before the instances are dropped. With a zero grace the instances are dropped at once and a queued `Result` is not applied.
- `Checkpoint` serializes and deserializes inside `#[serde(flatten)]` and internally tagged enums, and through `serde_json::Value`. `Checkpoint::from_json_slice` keeps the state text exactly. The `Deserialize` implementation reads the state as a `serde_json::Value`: object keys are sorted and an integer beyond 64 bits does not survive (a state with such a number must be read with `from_json_slice`). `PartialEq` compares the states as JSON values.
- `Checkpoint` deserialization refuses unknown fields (at the top level of the checkpoint).
- `resume`: a panic of `Fields::validate` or of the state deserialization while the state is rebuilt is refused as `ResumeError::StateRejected` with the panic text.
- `Observer::observe` is synchronous and runs in a task of the host's runtime: it must return quickly. An observer that blocks its thread stalls a `current_thread` runtime, and so the run with it.

### Known limits

- A panic of `Clone` of the host's `Fields` while the final state of a `RunEnded` is returned is not caught. The state is moved out without a clone when the run holds the only reference to it.
- A panic is caught only when unwinding is enabled.
