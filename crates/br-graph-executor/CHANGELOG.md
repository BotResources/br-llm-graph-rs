# Changelog

All notable changes to `br-graph-executor` are documented here. The git tag
`br-graph-executor/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.1.0 - 2026-10-26

First release: the executor of the graph family. It depends on `br-graph-contract` only, never on a library of nodes.

### Added

- `start` and `resume`: run a graph of `br-graph-contract` from a state and a list of nodes, or from a checkpoint.
- `RunConfig` (cancel token, grace period, optional checkpoint sink and observer), `Outcome` (`RunEnded`, `RunFailed`, `RunCancelled`) and `RunFailure`. The `cause` of `RunFailure::NodeFailed` is optional: `Some` when the `Failed` event of the node carried one, `None` when the executor itself counts a `Cancelled` without a cancel request as a failure.
- `Checkpoint`: plain serializable data (state, structure, names of the nodes that are not idle), `StartError`, `ResumeError`.
- `CheckpointSink`, `SinkFuture` and `SinkRefusal`; `Observer`, `Observation`, `ObservedEvent` and `OutcomeKind`.
- `RunFailure::SinkRefused { reason, checkpoint }`: the failure carries the checkpoint the sink refused (the one handed to the sink, which holds the state of its step). The checkpoint of the outcome stays the most current one and can be newer.
- `RunFailure::CheckpointNotBuilt`: the state of a run could not be serialized (error or panic) when a checkpoint was built. The checkpoint of the outcome then holds an empty string as its state (never valid JSON, so every `Fields` refuses it): `resume` returns `StateRejected`, never a fabricated state. The failure can be listed twice (once for a step, once for the outcome).
- A recorded failure is never lost as `RunEnded`: `RunEnded` is returned only when every node is idle and no failure was recorded. Every node idle with a recorded failure gives `RunFailed` when the executor triggered the token, and `RunCancelled` with the failure listed when the host did first. Such an outcome can carry a checkpoint that names no node: resuming it is refused as `NoNodes`.
- A `Result` already queued when the grace period ends is applied, and its edge evaluated, before the instances are dropped. With a zero grace the instances are dropped at once and a queued `Result` is not applied.
- `Checkpoint` has one wire form, exact and independent of the serde format: `{"state": "<json text>", "structure": ..., "nodes": [...]}`. The state is a JSON string that holds the compact JSON text of the state (the text of `State::to_json`; the checkpoint of a `CheckpointNotBuilt` outcome holds an empty string instead), so it is kept byte for byte: object key order and integers beyond 64 bits survive through every door (`Checkpoint::from_json_slice`, `serde_json::from_str`, `serde_json::from_value`, `#[serde(flatten)]`, internally tagged enums) and with any serde format. `Checkpoint::state_json` returns that text, and `PartialEq` compares it exactly (two states that differ only by key order are different).
- `Checkpoint` deserialization refuses unknown fields (at the top level of the checkpoint). `Checkpoint::from_json_slice` returns `ResumeError::Malformed` for any document it cannot read, also when `state` is not a string.
- `resume`: a panic of `Fields::validate` or of the state deserialization while the state is rebuilt is refused as `ResumeError::StateRejected` with the panic text.
- A grace period whose deadline cannot be represented, or is too close to the limit of `Instant` to be rounded up by the runtime timer (`Duration::MAX` for instance), counts as about 30 years: the deadline never falls back to a zero grace and never panics.
- A node, creation or edge error whose `Display` panics does not escape the run: the panic text is the reason of the `RunFailure`, and the cause is kept.
- `Observer::observe` is synchronous and runs in a task of the host's runtime: it must return quickly. An observer that blocks its thread stalls a `current_thread` runtime, and so the run with it.

### Known limits

- A panic of `Clone` of the host's `Fields` while the final state of a `RunEnded` is returned is not caught. The state is moved out without a clone when the run holds the only reference to it.
- A panic is caught only when unwinding is enabled.
