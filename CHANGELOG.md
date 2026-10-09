# Changelog

All notable changes to `br-llm-graph` are documented here. A single git tag
`v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings are plain `## X.Y.Z` — the release pipeline greps that exact
form to decide whether a version ships.

## Unreleased

## 0.3.0 - 2026-09-25

### Added

- `Limit`: a positive bound, given as a fixed number or read from an int
  configuration key. A key that is not declared as an int configuration key is
  refused at build (`LimitKeyMismatch`); a value below one is refused at run
  time (`LimitNotPositive`).
- `Map::max_concurrency`: at most that many bodies run at once; the results
  keep the item order. `None` runs every item at once, as before.
- `ToolNode::max_concurrency` and `ReactLoop::tool_concurrency`: at most that
  many pending calls of one tool node run at once; the results keep the call
  order. `None` runs every call at once, as before.
- `ReactLoop::round_limit` (`RoundLimit`, `OnLimit`): bounds the tool rounds of
  one agent turn. A round is one reply that calls tools plus the execution of
  all its calls; the count starts again with each new turn of the agent. When
  a reply asks for tools after the last allowed round:
  - `OnLimit::Error` fails the run with `ToolLimitReached`;
  - `OnLimit::Continue { node, flag }` answers each call with an error result
    saying it was not executed, sets `flag` to true, and calls the model once
    more with tool calls forbidden; a reply that still calls tools fails the
    run with `ToolLimitReached`;
  - `OnLimit::End { node, flag }` answers each call the same way, sets `flag`
    to true, and closes the turn with a step whose stop reason is
    `other(tool_round_limit)`, without calling the model; the loop goes on to
    `after`.

  `node` names the node the loop adds to do this. A `flag` that is not a bool
  state key is refused at build (`FlagKeyMismatch`).
- `Request::tool_calls` (`ToolCalls::Allowed`, `ToolCalls::Forbidden`): whether
  the model may call tools in this reply. The tools stay declared either way; a
  model adapter maps `Forbidden` to the provider's setting that disables tool
  calls.
- `std::error::Error::source` for `GraphError` (the fault of a failed node, a
  message error) and for `NodeFault` (the error a node returned, a refused
  update).
- Graph signature: `GraphBuilder::input(key)` and `GraphBuilder::output(key)`
  declare which state keys a graph takes and gives back; `Graph::signature()`
  lists them with their kinds (`Signature`). A key may be both. An undeclared
  key is refused at build (`UnknownKey`), a key declared twice too
  (`DuplicateInput`, `DuplicateOutput`). A graph without declarations has no
  inputs and no outputs.
- Defaults: `Kind::neutral()` (empty string, 0, 0.0, false, empty list, empty
  conversation) and `SchemaBuilder::state_with_default(key, kind, value)` for an
  explicit start value, checked at build (`KindMismatch`, `UnknownKey`).
  `Schema::default_value(key)` gives the start value of a key.
- `Graph::start_state(inputs)`: the start state of a run, from the given
  inputs plus the defaults of every other key. Refuses a declared input that is
  missing (`MissingInput`), a key that is not a declared input (`NotAnInput`),
  an unknown key, a value of the wrong kind, and an input given twice
  (`InputGivenTwice`). `State::new` is unchanged.
- Occurrences (`origin`): `Segment { node, index }`, `OccurrenceKey` (the path
  of a node occurrence through nested runs, empty at top level, written and
  serialized as `a/b[3]/c`), `RunId` (an opaque id supplied by the host) and
  `Origin` (run id and occurrence).
- `Context` knows where it runs: `with_run_id`, `run_id()`, `occurrence()`,
  `origin()`, `for_node(&NodeId)` and `child(Segment)`. `Context::new` gives an
  empty occurrence and the default run id. The run loop gives each node a
  context for its own occurrence.
- `SubGraph`: a node that runs another graph to its end. `SubGraph::call(graph)`
  then `.input(child_key, Input)` (`Input::From(parent_key)`,
  `Input::Config(parent_config_key)`, `Input::Const(value)`),
  `.config(child_config_key, Input)`, `.output(child_key, Output)`
  (`Output::Set(parent_key)`, `Output::Append(parent_list)`, renaming
  allowed), `.output_end_label(Output)` and `.on_failure(OnFailure)`;
  registered with `GraphBuilder::subgraph(id, call)`. The child starts from
  `Graph::start_state` with the mapped inputs and runs with a context for the
  calling node's occurrence; its state stays private and nothing is kept
  between two calls. `OnFailure::Propagate` (the default) fails the node with
  `SubGraphFailed`, whose `source()` is the child's `GraphError`. A nested run
  that stops before its end gives `SubGraphSuspended`.
- `OnFailure::Capture(Vec<CaptureUpdate>)`: a call whose child fails returns
  declared updates instead of its outputs, and the run goes on. Each
  `CaptureUpdate` is `Set` or `Append` into a parent key, from a
  `CaptureSource`: `Const(value)`, `From(parent_key)` (in a map body, the item
  key included) or `Reason` (the child's error message). Checked at build like
  outputs (`CaptureMismatch`). Inside a map the captured appends are
  forwarded in item order and the item is recorded as finished.
- Pending writes and resume: `PendingWrites` (occurrence key to
  `PendingEntry { updates, witness }`, with `insert`, `get`, `merge`) and
  `Checkpoint::pending`. A map item that finishes records its appends under its
  occurrence with the item as witness (`Context::record_item`) while the map's
  superstep is still open; `Context::recorded_item(item)` gives what an earlier
  attempt recorded only when it ran on the same item value, and the map uses
  it instead of running the item again. A record made on another value is
  ignored and replaced.
  Every node of a superstep that returns updates records them the same way
  under its own occurrence (`Context::record`), and a node recorded by an
  earlier attempt is not run again (`Context::recorded`), a map that finished
  as a whole included. Entries of a node are dropped once its superstep is
  applied. The checkpoint of a failure, a cancel or a pause carries the run's
  pending writes; `Session::resume` and `Context::with_pending(checkpoint.pending)`
  (for `run`) hand them back, so a resumed run skips the finished nodes and
  items. Pending writes belong to the run being resumed: its nodes, the items
  of its maps and of a map directly inside one of them (`m[0]/m[1]`).
- A called graph restarts whole. When a call starts, the pending entries below
  its occurrence are removed, and inside it nothing is recorded or reused
  (`record` and `record_item` do nothing, `recorded` and `recorded_item` find
  nothing), however deep: the child rebuilds its own state, so an entry made
  inside it may come from a value or a loop round the restart does not
  reproduce. The call's own result is recorded by the run or the map that
  holds it, so a map whose items call a graph still skips its finished items
  on resume.
- `Observer::recorded(origin, entry)` (defaulted): fires on every record, so a
  host can persist pending entries, witness included, as they come and insert
  them later under `origin.occurrence`.
- `Checkpoint::with_pending`; `Context::pending()` copies what a recorder
  holds.
- `Map::on_item_failure` (`ItemFailure`), what a map does when the body of an
  item returns an error:
  - `Finish` (the default): every item runs to its end and the finished ones
    are recorded, then the map fails with the first error in item order;
  - `FailFast`: no item starts after the first error; items already running
    finish and are recorded, then the map fails with the first error in item
    order among the items that ended; items that never started are not
    recorded;
  - `Capture(Vec<CaptureUpdate>)`: the item yields these appends instead,
    forwarded in item order and recorded like a result, so the map does not
    fail because of an item error. `From(key)` reads the item's own state,
    `Reason` is the error's message. A panic is not captured. Checked at build
    like a call's captures; a `Set` is refused (`MapBodySet`).
- Example `subgraph_map`: a map whose body calls a graph, with a resume.
- `Node::check(&self, &Schema)`, defaulted to accept: a node checks itself
  against the schema of the graph it runs in. `GraphBuilder::build` calls it on
  every node, however registered (`node`, `join`, `map`, `subgraph`), and
  refuses the graph with `InvalidNode { node, source }`, `source` being the
  reason. `SubGraph` checks its mappings, `Map` its list and item keys, its
  limit and its captures, then its body against the same schema. A node that
  wraps another forwards `check` to it.
- Reasons a call or a map is refused: `SubGraphInputUnmapped`,
  `SubGraphInputTwice`, `SubGraphNotAnInput`, `SubGraphSourceMismatch`,
  `SubGraphConfigUnmapped`, `SubGraphConfigTwice`, `SubGraphNotAConfig`,
  `SubGraphConfigMismatch`, `SubGraphNotAnOutput`, `SubGraphTargetMismatch`,
  `CaptureMismatch` and `MapBodySet`, each naming the key.

### Changed

- `NodeFault::Returned` carries the error value the node returned
  (`NodeError`) instead of its text, so a host can downcast it to its own error
  type.
- `Map`, `ToolNode`, `ReactLoop` and `Request` gain the fields above; struct
  literals must set them.
- Depends on `br-llm-messages` 0.2.0: the body of a framed entry is rendered
  verbatim, no longer escaped.
- Every `Observer` method receives an `&Origin` first. Events of the run loop
  carry the run's occurrence (empty at top level), events a node emits carry
  the node's occurrence, so events of a nested run are told from the
  parent's.
- `Context` gains private fields; build it with `Context::new`.
- `run` gives each run its own pending-writes recorder, seeded with what the
  given context holds, so two runs sharing one context never see each
  other's records.
- `Checkpoint` gains the field `pending`, read as empty from checkpoints
  written before it and not written when empty; build one with
  `Checkpoint::new`.
- `Map` is `{ list, item, body, max_concurrency, on_item_failure }`: the
  `output` and `results` fields are gone. The body runs on the state with `item` set, in a
  context for its own occurrence (`m[i]`); its updates are not applied to that
  state. Each must be an `Update::Append` to a list of the graph and the map
  forwards them, in item order whatever the completion order, so one body may
  append to several lists and they stay aligned. Any other update fails the
  map (`MapBodyNotAppend`), never silently dropped; this includes a `Set`
  output or capture of a `SubGraph` body, which its own `check` cannot see as a
  map body. A map run outside a graph, whose context names no node, fails with
  `MapWithoutOccurrence`.
- A superstep is applied as a whole. When one of its nodes fails or panics,
  its updates are refused, or its edges or the end label cannot be resolved,
  nothing of it reaches the state: the failure checkpoint holds the state from
  before the superstep (plus the inputs received meanwhile), the cursor of the
  whole superstep and the pending entries of the nodes that finished. Before,
  the updates of the nodes that succeeded were applied, and a resume applied
  them a second time. A node whose updates are refused is not kept pending, so
  a resume runs it again. Observers see `applied` only for a superstep that is
  applied.
- `Map` runs its bodies in a rolling window: at most `max_concurrency` at
  once, a new one starting as soon as any running one finishes, never held by
  a slow earlier item.
- `GraphError::MapKeyMismatch` is `{ list, item }` and covers the list and
  item keys only; like every build-time reason of a node, it reaches the
  caller inside `InvalidNode`. A `Map` registered with `node` or `join` is now
  checked too.
- `Schema` gains the public field `defaults` (serialized only when not empty,
  so earlier schemas and checkpoints still load) and no longer implements
  `Eq`, since a default is a `Value`.

## 0.2.0 - 2026-09-22

### Removed

- `Message`, the inbox item type, is no longer public. It is an internal
  detail of the run loop: a runner drives a session through
  `Sender::send`/`pause`/`resume`/`cancel` and hands the `Inbox` to `run` or
  `Session`, never constructing or matching an inbox item itself.

## 0.1.0 - 2026-09-22

### Added

- Repository scaffold: crate manifest, governance files (LICENSE,
  CONTRIBUTING, SECURITY, SUPPORT, PR template, issue-template config),
  `.gitignore`, and `deny.toml`.
- CI (`ci.yml`): fmt + clippy + test, MSRV 1.89 build, `cargo-deny`,
  `cargo-machete`, `cargo semver-checks`, changelog + README-pin check,
  shellcheck, and trufflehog secret scan.
- CD (`release-tags.yml`): auto-tag and release the crate version on merge to
  `main`, inert while the version is the `0.0.0` scaffold.
- Value objects (`value`): validated `Key`, `NodeId`, `EndLabel` newtypes
  (`[a-z][a-z0-9_]*`), serde through `try_from` so illegal JSON is refused.
- Typed state (`state`): `Value` (with a `Finite` float that refuses NaN and
  infinities), `Kind`, `Schema` + `SchemaBuilder`, `State` and `Config` with
  present/undeclared/kind checks at construction and re-validated at load,
  typed getters, `State::derive`, canonical JSON at `SCHEMA_VERSION`
  `br-llm-graph/1`.
- Updates (`update`): `Set`, `Append`, `Input`, `PushTurn`, `PushStep`,
  `PushResult`; atomic `State::apply_batch` with the `SetConflict` rule.
- Graph (`graph`): `Node`/`Edge` traits, `NodeFuture`, `Target`, `Context`,
  `IdSource`, the `FnNode`/`FnEdge`/`Always` adapters, the `Map` fan-out node,
  and `GraphBuilder`/`Graph` with build-time refusals (duplicate id, unknown
  entry, missing edge, edge from unknown node, duplicate edge, map key
  mismatch).
- Run loop (`run`): the Pregel superstep engine with static fan-out, the single
  join rule, per-node application in declaration order, caught node panics, an
  edge to a node the graph does not contain refused with `UnknownNode` rather
  than a silently dropped branch, the runtime-neutral inbox (`Sender`/`Inbox`),
  `Pause`/`Resume`/`Cancel` commands, `Cursor`, `Checkpoint`, `Outcome`, and
  `RunFailure`.
- Session (`session`): `Session` with `new`/`resume`/`sender`/`serve`/
  `run_once`, queueing inputs during a run and relaunching after an end.
- Observer (`observe`): the `Observer` trait with empty defaults and
  `NoopObserver`.
- React module (`react`): the `Model` and `Tool` traits, `Request`,
  `OutputMode`, `ToolSpec`, `StreamSink`, `ToolOutput`; the `wire`, `complete`,
  `structured`, `pending_calls` (`state`/`key`/`author` in, borrowed calls out),
  `pending_unsafe_calls` helpers; `LlmNode` (with `Source`), `ToolNode`, and
  `ReactLoop` with a strict tool-set partition check (a tool node tool the LLM
  node does not declare is rejected) and a fail-closed loop edge (a pending call
  no tool node runs is refused, never left to livelock).
- Errors (`error`): a single `GraphError` type with a hand-written `Display`
  and `NodeFault` for node failures, including `ToolNotDeclared` and
  `PendingToolUnsatisfiable`.
- Run loop: a `Cancel` that arrives while a fast superstep is completing returns
  the still-unmutated state and reruns the superstep on resume, so no node
  update is applied twice; an inbox input already drained during that superstep
  is folded into the `Cancelled`, `Paused` and node-failure checkpoints, so an
  input the lib has taken ownership of survives `Session::resume`.
- Examples: `react_agent`, `react_goal_loop`, `generator_critic`,
  `background_task`, `external_message`, `skills`, `rehydration`, over a shared
  scripted-model and fake-tool harness in `examples/common`.

