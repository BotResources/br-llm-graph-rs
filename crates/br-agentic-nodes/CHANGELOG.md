# Changelog

All notable changes to `br-agentic-nodes` are documented here. The git tag
`br-agentic-nodes/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.1.0 - 2026-10-29

First release: the agent nodes and their building blocks. It builds on `br-agentic-contract`, `br-graph-contract`, `br-llm-messages` and `br-llm-clients-core`.

### Added

- `ReactAgent<F>`: a node of kind `agentic` and subkind `react_agent`. One node runs the whole loop: step, tool round, drain, next step, until a final response. Builder: name, `Ports<F>`, `FallbackList`, `Settings` (step limit, `Timeouts`, `WebAccess`, `FileSystemMode`) and optionally `with_author`.
- `GeneratorCritic<F>`: a node of kind `agentic` and subkind `generator_critic`. A generator (a ReAct pass) and a critic (one structured call, no tool, no AI turn) share one thread. `GeneratorCriticSpec` holds the two `Ladder`s, the `RoundLimit` and the `CriticRequestSource` (`Fixed` or a `CriticRequestPort`); `with_authors`, `with_generator_author`, `with_critic_author`, `with_step_limit_ending(StepLimitEnding::{WithoutCriticCall, WithCriticCall})` (default: without) and `with_critic_thinking_field`.
- `Outcome` (last response, step-limit flag, verdict) with the `OutcomePort` and the `PromptPort` that the runner gives. `Ports<F>` groups the ports of the contract with these.
- The building blocks, public: `Worker` and `WorkerParts` (`step`, `tool_round`, `drain`, `close_turn`, `load`, `steps_remaining`), `Slot` (the settled client of a fallback list), `FallbackList`, `Ladder`, `ModelClient` (with `expecting_thinking`), `check_thinking` / `holds_thinking` (the thinking rule), `warning_text` / `WARNING_WINDOW` (the warning near the limit), `mail_tools` with `SEND_MAIL` and `GET_MAIL_FILES`, and `FileSystem`, `SandboxPath`, `WebGuard`.
- `Fault`, `CallFailure` and `Stop`: the faults of an instance with their source chain.

### Behaviour to know

- A client that should think is declared by the runner (`ModelClient::expecting_thinking`): `br-llm-clients-core` 0.5 carries no such report on a model response, so the node cannot read one.
- `FileSystem` and `WebGuard` are values. A `ToolCode` receives only its arguments, a cancel view and a result stream, so a tool cannot reach the file system or the web guard of the node; the node loads the files at the start and saves them before `Result`, and the runner wires any tool access.
- The critic request is added to the projected thread of the critic for one call only: into the last input turn when it shows no agent output, else in a new input turn. It is never stored.
- In a generator-critic the round counter goes back to 0 when a drain stores items, also a drain inside a pass; the critic of that round keeps the ladder entry read at the start of the pass.
- `get_mail_files` acts only on ids whose entry in the projected thread is an incoming agent mail; any other entry or an absent id gives a result with the error flag.
- After a tool round the drain stores the queue into the open AI turn; at the start of a pass and after a final response it stores it where `store_at_boundary` puts it for an agent with no open AI turn (the last real input turn, else a new one).
