# br-agentic-nodes

The agentic nodes of a graph: a ReAct agent and a generator-critic, and the building blocks they are made of. A runner builds a node with the ports of `br-agentic-contract` and the model clients it already built; the node runs the loop.

The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

## Start here

- Entry point: `ReactAgent::new(name, ports, settings, ReactSpec { fallback, outcome, author })` or `GeneratorCritic::new(name, ports, settings, GeneratorCriticSpec { .. })`. Both are `Node<F>` of kind `agentic`. `Ports::new(thread, agent, access, prompt, tools, workspace)` groups the ports of the contract with the `PromptPort`; `with_inbox` gives the inbox, and a node with an inbox gets the mail tools.
- A model client is a `ModelClient`: the client with the runner's `ThinkingExpectation` (`Expected` or `NotExpected`; no default). A `FallbackList` holds at least one; a `Ladder` holds at least one list.
- Ports of this crate, all asynchronous (`PortFuture`, `PortError`): `PromptPort` (text), `AuthorPort` (text, refused by `br-llm-messages` when blank), `ReactOutcomePort`, `GeneratorCriticOutcomePort`, `CriticRequestPort`, `ProgrammaticCriticPort`. A prompt, an author name from a port or a critic-request text that is blank is a fault of the instance.
- Author names: `AuthorSource::Fixed(Author)` or `AuthorSource::Port(..)`, one per agent.
- Outcomes: `ReactOutcome` (response, steps made, step-limit flag) and `GeneratorCriticOutcome` (response, `Judgement::{Validated, Rejected, NotJudged}`, iterations, step-limit flag).
- Generator-critic settings: `RoundLimit`, `StepLimitEnding::{WithoutCriticCall (default), WithCriticCall}`, `RoundLimitEnding::{FinalGeneration (default), KeepRejectedGeneration, Fail}`, `CriticRequestSource::{Fixed, Port}`, and optionally `ProgrammaticCritic { port, refusal_limit, author }` through `with_programmatic_critic`. Agent labels: `agent` for a ReAct agent; `generator`, `critic`, `programmatic_critic` for a generator-critic.
- Tests: the in-memory ports of `br-agentic-contract` (`memory`) are enough to run a node; `tests/nodes/common` holds a scripted model client and a rig.

## The building blocks

Public, so that a runner author can write a node of their own: `Worker` (opened with `WorkerParts`; `step`, `tool_round`, `drain_at_start` (the awaited inbox read of Start, then a drain), `drain`, `close_turn`, `conversation`, `steps_made`, `steps_remaining`, and `mail_tools`: the mail tools of a worker that has an inbox, bound to its conversation, empty without an inbox, a fault without an author), `Slot` (the settled client of a fallback list), `check_thinking` and `holds_thinking`, `warning_text` and `WARNING_WINDOW`, the names `SEND_MAIL` and `GET_MAIL_FILES`, `FileSystem` and `SandboxPath`, and `Fault`, `CallFailure` and `Stop`.

## What the node does

- The instance reads the conversation once, at Start, through the thread access, and keeps a copy that `br-llm-messages` rules. Every addition goes to the copy first and, only when the copy accepts it, to the thread access, awaited before the next one. A thread access that keeps nothing gives the same result.
- A model step projects the copy for the agent. The warning near the limit (at most 10 steps left, after a tool run) and the critic request are added to a projection only, never stored.
- The thinking rule (a client declared `Expected` must give a thinking block) applies to the first model call of a ReAct instance, the first step of each pass of a generator, and each call of the critic. A failed call there is handled by the fallback.
- The inbox is read at Start with an awaited read, then by one background read at a time, started at each boundary (start of a pass, after a tool run that left no provider continuation and no provider execution pending, after the closure of a final response). The read is never awaited at a boundary; its items join the queue when it has finished. A read or a mark that fails is not a fault.
- The mail of an inbox item is built by this crate: the text carries the sender, the recipients, the subject, the body and the attachment names, and each text written by the sender is quoted and escaped.
- No tool can reach the file system of the node: a `ToolCode` gets only its arguments, a cancel view and a result stream. `Result` saves back what Start loaded.
- A fault closes the open AI turn of the node `Failed`; a cancel closes it `Cancelled`; both discard the open objects, drop the running read and ignore the errors of these closures and discards.

## Why

| Thing | Why it is the way it is |
|---|---|
| `ModelClient::new` has no default `ThinkingExpectation` | `br-llm-clients-core` reports no thinking; the runner declares it, and the node never reads client settings |
| Ports give `String` for a prompt and an author name | the intent makes a blank value a fault of the instance, so the instance checks it |
| A background read joins the queue only when it has finished | a hand-over never waits for a read; items that come later wait for the next boundary |
| Mail text quotes each sender text with `"`, `\` and control characters escaped | no subject, body, author name or attachment name can end its own part of the text block |
| The recipient author name is shown only for the agents of the node | the mail item carries identities only; the node knows only its own agents |
| `get_mail_files` reads a snapshot of the incoming entry ids, refreshed before each tool round | a tool call runs apart from the instance and cannot borrow the copy; only drains add incoming entries, and none runs during a tool round |
| The programmatic critic and the critic message are AI turns completed without being opened | the intent stores them as finished responses that no stream shows |
| `with_critic_thinking_field` is gone | the shape of the critic answer follows the runner's declaration for each client of the fallback |
| The web access is held, not enforced | a `ToolCode` receives only its arguments, a cancel view and a result stream, so the node has no hook on what a tool reaches |
