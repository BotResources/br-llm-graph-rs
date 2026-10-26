# Changelog

All notable changes to `br-flow-nodes` are documented here. The git tag
`br-flow-nodes/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.1.0 - 2026-10-26

First release: two generic programmatic nodes for the control of a graph,
built on `br-graph-contract` 0.1.0 only.

### Added

- `Merger`: a node of kind `programmatic` and subkind `merger`, made from a
  name, a ready function and a spent function. It calls the spent function
  first, then the ready function, on the state of each input alone; when the
  ready function gives true it closes its input end, reads the inputs it had
  accepted and sends one `Result` with the empty update. A triggered cancel
  token closes the input end and sends `Cancelled`.
- `Merger::late_input(LateInput)` sets the policy; `LateInput` (`Silent` by default, `Fail`, `FireAgain`): what a merger does
  with an input that the spent function calls spent; `Fail` closes the input
  end and sends `Refused` with a reason.
- `MapReduce`: a node of kind `programmatic` and subkind `map_reduce`, made
  from a name, a map function, an internal node, a limit (`NonZeroUsize`) and a
  reduce function, with an optional internal size limit set by
  `MapReduce::internal_size_limit` (default `DEFAULT_STATE_LIMIT`). It works on the state of the last input it had
  accepted, creates the internal instances one at a time in list order (at
  most `limit` live, a fresh UUIDv7 each and a view of the one child cancel token
  of the outer instance), applies each
  internal `Result` to its internal state and sends the update of the reduce
  function, which receives the final states in map order.
- `MapReduceError`: the cause of the `Failed` of a map-reduce instance, one
  named case per fault (invalid initial value, instance not created, instance
  id mismatch, input refused, internal failed, internal refused, internal
  cancelled while the child token is not triggered, internal vanished, update
  rejected), each with the position of the internal state; `#[non_exhaustive]`.
- The `Display` of `MapReduceError::InstanceNotCreated` and of `MapReduceError::InternalFailed` ends with `: {cause}`, the text of their `cause`, so the reason of a `RunFailure` built from the top-level `Display` shows the text of the internal node; the `cause` stays the `source`.
- A panic in a function of a node (the map or reduce function of `MapReduce`, the ready or spent function of `Merger`) ends the instance with no terminal event: the executor reads `EventEndError::Vanished`, never `Failed`.
- An internal node that closes its input end before its input is read gives `MapReduceError::InputRefused`, even after it has sent a terminal event.
- A triggered cancel token before the first input gives `Cancelled` and closes the input end, for `MapReduce` and for `Merger`.
- A dropped map-reduce instance (either end) triggers the child cancel token and drops its internal instances.
