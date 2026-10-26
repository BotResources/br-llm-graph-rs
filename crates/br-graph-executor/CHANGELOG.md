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
- `RunFailure::CheckpointNotBuilt`: the state of a run stopped serializing when a checkpoint was built.
