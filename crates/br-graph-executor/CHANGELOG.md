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
- `RunFailure::CheckpointNotBuilt`: the state of a run could not be serialized (error or panic) when a checkpoint was built. The checkpoint of the outcome then holds `null` as its state: a host must not resume it. The failure can be listed twice (once for a step, once for the outcome).
- `Checkpoint` deserialization refuses unknown fields.
- `Observer::observe` is synchronous and runs in a task of the host's runtime: it must return quickly.
