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
- The `Display` of `MapReduceError::InstanceNotCreated` and of `MapReduceError::InternalFailed` ends with `: {cause}`, the text of their `cause`, so the reason of a `RunFailure` built from the top-level `Display` shows the text of the internal node; the `cause` stays the `source`. The executor reads only the top-level `Display`; a host that walks `source()` as well prints the cause twice (accepted design).
- A panic in the map or reduce function of `MapReduce`, or in the ready or spent function of `Merger`, ends the instance with no terminal event: the executor reads `EventEndError::Vanished`, never `Failed`. A panic in the work of an internal instance (its own task) gives `Failed` with `MapReduceError::InternalVanished`. A panic is caught only when unwinding is enabled.
- A panic in the creation of an internal instance (the call to `create` or the poll of its future) gives `Failed` with `MapReduceError::InstanceNotCreated` (the cause is an opaque cause holding the panic text). A panic in `State::apply` of an internal state (so also in the `Clone`, `validate` and `Serialize` of its fields, and also when `validate` panics during an apply) gives `Failed` with `MapReduceError::UpdateRejected`; a panic in `State::create_with_limit` of an initial internal state gives `Failed` with `MapReduceError::InvalidInitialValue`. Each carries the position and the panic text (the message when the payload is a `&str` or a `String`, else `panic without a text message`). The other internal instances are dropped as for any fault. The payload of a caught panic is read for its text and then dropped under a guard; a payload whose `Drop` panics is leaked and cannot escape. Once the creation of an internal instance has given its result or its panic, a later poll gives `Pending` and never polls the inner future again.
- An internal node that closes its input end before it receives its one input gives `MapReduceError::InputRefused`, even if it has already sent a terminal event: the map-reduce node does not read the event before it gives the input.
- A triggered cancel token before the first input gives `Cancelled` and closes the input end, for `MapReduce` and for `Merger`.
- A dropped map-reduce instance (either end) triggers the child cancel token and drops its internal instances.
