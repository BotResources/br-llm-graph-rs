# Changelog

All notable changes to `br-agentic-nodes` are documented here. The git tag
`br-agentic-nodes/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.2.0 - 2026-10-31

Rework to the sealed intent of minor 0.2, on `br-agentic-contract` 0.2.0.

### Removed

- `Fault::NoAuthor`, `Fault::Thread`; the `From` impls of `Fault` for the 0.1 `PortFault`, `ThreadError` and `BoundaryError`.
- `Outcome` (with `response`, `step_limit_reached`, `verdict`) and `OutcomePort`; `Ports::new` no longer takes the outcome port.
- `ReactAgent::with_author`, `ReactAgent::restart`, `GeneratorCritic::restart`, `GeneratorCritic::with_authors`, `with_generator_author`, `with_critic_author`, `with_critic_thinking_field`. The shape of the critic answer follows the `ThinkingExpectation` of the client of each attempt.
- `Worker::new`, `Worker::load`, `WorkerParts::author`, `WorkerParts::system`, `WorkerParts::tools`.
- The free function `mail_tools`; a worker gives its mail tools with `Worker::mail_tools`.

### Changed

- Every port returns `PortFuture` with `PortError`; `Fault::Port` holds a `PortError`, `Fault::Boundary` holds the 0.2 `BoundaryError`.
- `PromptPort::prompt` and `CriticRequest::text` use `String`; a blank value is a fault.
- `CriticRequestPort::request` takes the last model response of the generator; it is called once per round.
- `Ports::new(thread_port, agent_port, thread_access, prompt_port, tools_port, workspace_port)`.
- `ReactAgent::new(name, ports, settings, ReactSpec)`.
- `WorkerParts::inbox` is an `Option<Arc<dyn InboxPort>>`.
- `Worker::drain` gives `Drained { stored, request_stored }`.
- `Worker::close_turn` takes `&mut self` and an `&AiTurnRef`.
- `GeneratorCriticSpec` gains `generator_author`, `critic_author` and `outcome`.

### Added

- `Worker::open` (async, reads the conversation once), `Worker::conversation`, `Worker::steps_made`, `Worker::with_author`, `with_system`, `with_tools`, `Worker::mail_tools`, `Worker::drain_at_start`; `Drained`.
- `ReactSpec { fallback, outcome, author }`, `AuthorSource::{Fixed, Port}`, `AuthorPort`.
- `ReactOutcome` (`response`, `steps`, `step_limit_reached`) with `ReactOutcomePort`; `GeneratorCriticOutcome` (`response`, `judgement`, `iterations`, `step_limit_reached`) with `GeneratorCriticOutcomePort`; `Judgement::{Validated, Rejected, NotJudged}`.
- `RoundLimitEnding::{FinalGeneration (default), KeepRejectedGeneration, Fail}` and `GeneratorCritic::with_round_limit_ending`.
- The programmatic critic: `ProgrammaticCritic { port, author, refusal_limit }`, `ProgrammaticCriticPort`, `ProgrammaticVerdict::{Accept, Refuse}`, `ProgrammaticRefusalLimit`, `PROGRAMMATIC_CRITIC_LABEL`, `GeneratorCritic::with_programmatic_critic`.
- `Fault::{Session, ProgrammaticRefusal, BlankExplicitReason, CriticAtRoundLimit}`, `InvalidLimit::Refusals`.
- The crate README (also the crate documentation).

### Behaviour

- The conversation is read once at Start; the instance applies each write to its copy first and then hands it to the thread access.
- The queue is read at Start with an awaited read and later by one background read at a time, never awaited at a boundary; an error of a read or of a mark is not a fault.
- The thinking rule applies only to the calls the fallback covers; a refusal, a filter or an exhaustion without tool call is exempt.
- The round counter and the programmatic-refusal counter go back to 0 only when a request item is stored.
- All results of a tool run are opened before any call starts. Inbox errors in the mail tools are error results, never faults.
- The critic projection drops agent mails and runtime notices from input turns.
- A failed discard at `Result` is a fault of the instance, as `br-agentic-contract` 0.2.0 rules; this crate has no code for it.
- Web access is held in the settings but not enforced by this crate. No tool can reach the file system of the node: a `ToolCode` gets only its arguments, a cancel view and a result stream.
- All calls of a tool run start together: a call to a tool the node does not hold gets its error result after the real calls have started.
- `get_mail_files` gives each attachment name quoted, like the mail text, so a name written by the sender cannot forge a line of the result.

## 0.1.0 - 2026-10-29

First release: the agent nodes and their building blocks. It builds on `br-agentic-contract`, `br-graph-contract`, `br-llm-messages` and `br-llm-clients-core`.

### Added

- `ReactAgent<F>`: a node of kind `agentic` and subkind `react_agent`. One node runs the whole loop: step, tool round, drain, next step, until a final response. Builder: name, `Ports<F>`, `FallbackList`, `Settings` (step limit, `Timeouts`, `WebAccess`, `FileSystemMode`) and optionally `with_author`.
- `GeneratorCritic<F>`: a node of kind `agentic` and subkind `generator_critic`. A generator (a ReAct pass) and a critic (one structured call, no tool, no AI turn) share one thread. `GeneratorCriticSpec` holds the two `Ladder`s, the `RoundLimit` and the `CriticRequestSource` (`Fixed` or a `CriticRequestPort`); `with_authors`, `with_generator_author`, `with_critic_author`, `with_step_limit_ending(StepLimitEnding::{WithoutCriticCall, WithCriticCall})` (default: without) and `with_critic_thinking_field`.
- `Outcome` (last response, step-limit flag, verdict) with the `OutcomePort` and the `PromptPort` that the runner gives. `Ports<F>` groups the ports of the contract with these.
- The building blocks, public: `Worker` and `WorkerParts` (`step`, `tool_round`, `drain`, `close_turn`, `load`, `steps_remaining`), `Slot` (the settled client of a fallback list), `FallbackList`, `Ladder`, `ModelClient` (built with a required `ThinkingExpectation`), `check_thinking` / `holds_thinking` (the thinking rule), `warning_text` / `WARNING_WINDOW` (the warning near the limit), `mail_tools` with `SEND_MAIL` and `GET_MAIL_FILES`, and `FileSystem` and `SandboxPath`.
- `Fault`, `CallFailure` and `Stop`: the faults of an instance with their source chain.

### Behaviour to know

- A client that should think is declared by the runner; `br-llm-clients-core` gives no such report. The declaration is required: `ModelClient::new` takes a `ThinkingExpectation` (`Expected` or `NotExpected`) and has no default. The node never reads the settings of a client.
- The sealed intent asks the instance to enforce the web access and the sandbox. Version 0.1.0 holds the web access and the file system mode, loads the files at the start and saves them before `Result`, and enforces neither the web access nor the sandbox. A `ToolCode` receives only its arguments, a cancel view and a result stream, so no tool can reach the file system of the node. Until a later minor, the runner must give only tools that respect the web access.
- The critic request is added to the projected thread of the critic for one call only: into the last input turn when it shows no agent output, else in a new input turn. It is never stored.
- In a generator-critic the round counter goes back to 0 when a drain stores items, also a drain inside a pass; the critic of that round keeps the ladder entry read at the start of the pass.
- `get_mail_files` acts only on ids whose entry in the projected thread is an incoming agent mail; any other entry or an absent id gives a result with the error flag.
- After a tool round that leaves the AI turn at a boundary (no outstanding runtime call, no pending provider execution, no provider continuation) the drain stores the queue into the open AI turn; after any other tool round nothing is stored until the next boundary; at the start of a pass and after a final response it stores it where `store_at_boundary` puts it for an agent with no open AI turn (the last real input turn, else a new one).
