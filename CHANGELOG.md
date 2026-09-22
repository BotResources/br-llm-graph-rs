# Changelog

All notable changes to `br-llm-graph` are documented here. A single git tag
`v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings are plain `## X.Y.Z` — the release pipeline greps that exact
form to decide whether a version ships.

## Unreleased

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

