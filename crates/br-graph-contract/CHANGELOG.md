# Changelog

All notable changes to `br-graph-contract` are documented here. The git tag
`br-graph-contract/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings are plain `## X.Y.Z` — the release pipeline greps that exact
form to decide whether a version ships.

## Unreleased

## 0.1.0 - 2026-10-23

First release: the contract of the graph family, with no run logic, no
executor and no node library.

### Added

- `Fields`: the conditions on the runner's data (serializable, copyable,
  comparable, self-validating, with an update type that has an empty update
  and that the fields apply to themselves). `State` holds the fields behind
  three checked operations: `create`, `apply` (all or nothing, the first
  state is untouched) and `rebuild`, each refused as `InvalidState` or
  `ApplyError` with a `Reason`, under a size limit on the JSON form of the
  fields (`DEFAULT_STATE_LIMIT`, 128 KiB, a positive limit given by the
  runner).
- `Node` (name, kind, subkind, async creation of an `Instance` from a
  `CancelView` and an `InstanceId`), `PROGRAMMATIC`, `Edge`, `Target` and
  `END`.
- `Instance`, built with `Instance::channel`: the executor's `InputEnd`
  (`give`, never waits, refused as `InputClosed` after the close) and
  `EventEnd` (`recv`), the node's `InputReader` (`next`, `close`, `drain`) and
  `EventSender` (`finish` sends exactly one terminal `Event`, `dropped`).
  `Event` is `Result`, `Failed`, `Refused` or `Cancelled`.
- `InstanceId`: a UUIDv7 given by the creator, refused as `InvalidInstanceId`
  otherwise.
- `CancelToken`, `CancelHandle`, `CancelView`: a cooperative, runtime-free
  signal with child tokens.
- `GraphBuilder` and `Graph`: nodes, edges, entries and a version validated
  as a whole (`BuildError`, one named case per refusal), the checked edge
  evaluation (`EdgeError`) and `Graph::structure`.
- `Structure`: the graph as plain, sorted, serializable data, rebuilt from
  JSON through the same build rules (`StructureError`).
