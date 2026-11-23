# br-agentic-contract

What every agentic node has in common, in hexagonal terms.

- A **port** is a trait of this crate. The runner (or the harness it uses) gives the ports to a node when it builds it.
- An **adapter** is what a runner library (for example `br-runner-threads`, `br-runner-jobs`) implements for a port.
- The node libraries use the ports and the types of this crate; they never import an adapter. The node library defines no port: all ports of the node are here.
- This crate holds no logic of a node: the session, the storing at a boundary, the queue, the ending functions, the documents function and their boundary error are in `br-agentic-nodes`. The only rules it runs are the five public `apply_*` functions.

The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

## Start here

To write an adapter:

- Entry point: pick the port, implement its trait, return `PortError::Refused` only where the port is said to refuse, and `PortError::Fault` for anything else.
- Skeleton: `tests/contract/adapter_example.rs` is a compile-checked skeleton of four ports (its incoming port lists nothing and marks nothing; `src/memory/incoming.rs` is a working one). The agent port and the incoming port are shown below as doctested snippets.
- Imports: the types of `br-graph-contract` that a port signature names are re-exported by this crate, with `Event`, `InputReader` and `Node`; the ports need no `br-llm-clients-core` import except for the types they name (`StreamSink` is re-exported; `Tool::new` takes a `ToolSpec` and the output schema port gives a `StructuredShape`, neither re-exported).
- Thread access: the hardest port. `tests/contract/forgetful.rs` is a compile-checked thread access that accepts every call and keeps nothing; `src/memory/thread.rs` and `src/memory/streams.rs` show one that keeps what it receives. `ThreadPort` gives the reference of the thread; `ThreadAccess` reads the conversation and receives the additions.

```rust
use br_agentic_contract::{AgentLabel, AgentPort, Fields, InstanceId, PortError, PortFuture};
use br_llm_messages::{AgentId, Uuid};

struct SavedAgents { saved: std::sync::Mutex<std::collections::HashMap<String, AgentId>> }

impl<F: Fields> AgentPort<F> for SavedAgents {
    fn agent<'a>(
        &'a self,
        _snapshot: &'a F,
        _instance: InstanceId,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<AgentId, PortError>> {
        Box::pin(async move {
            let mut saved = self.saved.lock().map_err(|_| PortError::fault("poisoned"))?;
            if let Some(agent) = saved.get(label.as_str()) {
                return Ok(*agent);
            }
            let agent = AgentId::from_uuid(Uuid::now_v7()).map_err(PortError::fault)?;
            saved.insert(label.as_str().to_owned(), agent);
            Ok(agent)
        })
    }
}
```

This is an incoming port that has no item. A real one lists the unread items of the agent and keeps the read state.

```rust
use std::num::NonZeroUsize;

use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingPort, PortError, PortFuture,
};
use br_llm_messages::EntryId;

struct Empty;

impl IncomingPort for Empty {
    fn items<'a>(
        &'a self,
        _context: &'a IncomingContext,
        _after: Option<&'a Cursor>,
        _limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        Box::pin(async { Ok(IncomingBatch { items: Vec::new(), next: None }) })
    }

    fn mark_read<'a>(
        &'a self,
        _context: &'a IncomingContext,
        _item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async { Ok(()) })
    }
}
```

## The ports

All ports are public, object-safe (`dyn ThreadPort<F>`, `dyn IncomingPort`, ... compile), `Send + Sync`, and asynchronous: a method returns a `PortFuture`, a boxed `Send` future that this crate owns. The instance never retries a call to a port. A failure of a port other than the incoming port is a fault of the instance; no error of the incoming port is.

| Port | What it does |
|---|---|
| `ThreadPort<F>` | state snapshot to a `ThreadRef` (a UUIDv7) |
| `AgentPort<F>` | snapshot, instance id and `AgentLabel` to an `AgentId` |
| `ToolsPort<F>` | snapshot and a `&RunContext` to the `Tool`s of the instance |
| `ThreadAccess` | `read` the conversation once; every other operation receives a complete addition: `add_input_turn`, `add_entry`, `close_turn`, `open_*` (gives a stream), `complete_*`, `discard_*` |
| `IncomingPort` | `items` (batches and cursor) and `mark_read`, for an `IncomingContext` (instance, thread, agent); no wait, no send, no documents |
| `PromptPort<F>` | snapshot and label to the prompt text |
| `OutputSchemaPort<F>` | snapshot and label to an optional `StructuredShape` |
| `AuthorPort<F>` | snapshot and label to the author name of the agent |
| `ReactOutcomePort<F>`, `GeneratorCriticOutcomePort<F>` | snapshot and the outcome to the update of the fields |
| `CriticRequestPort<F>` | snapshot and the model response to a `CriticRequest` (text and human) |
| `ProgrammaticCriticPort<F>` | snapshot, the model response and the refusals in a row to a `ProgrammaticVerdict` |

`PortError` is the one error of every port: `Refused(Reason)` (a reason as text, an expected error) or `Fault(Cause)` (an opaque cause). The incoming port has no operation that refuses a request, and a refusal by the thread access is a fault of the instance.

`RunContext` is `#[non_exhaustive]` and made by `RunContext::new(instance, thread, agent, author, conversation)`: the instance id, the thread reference, the agent identity of the main agent, the author name of the main seat and the conversation id that the thread access gave to the instance (the source conversation of a mail it sends; it is not the thread). The mail gate is not in it. The instance calls the tools port once.

The incoming items are `IncomingItem::Mail(MailItem)` or `IncomingItem::Entry(EntryItem)`; `EntryItem` wraps a human message, a runtime notification or a runtime notice of `br-llm-messages` (`EntryItem::new` refuses any other entry with `InvalidEntryItem`; the wrapper has a name of its own so that it does not clash with `IncomingEntry`). `Cursor` and the mail types (`NewMail`, `MailItem`, `MailDocument`, `Attachment`, `DocumentRef`, `MailId`) keep their names.

## Rules

`apply_input_turn`, `apply_entry`, `apply_closure`, `apply_response` and `apply_results` apply one addition to a `Conversation` under the rules of `br-llm-messages`. They are public because the in-memory thread access needs the same rules as a real one, and a copy in a node library would drift. They give `ApplyError`: `Rules(Error)` (the rules refuse the addition, the conversation is unchanged) or `WrongTurn(TurnId)` (the turn is not the AI turn of that agent and author, or a closure names an AI turn that does not exist). `ApplyError` is `#[non_exhaustive]`; the session error of `br-agentic-nodes` converts from it.

## Outcomes

`ReactOutcome` and `GeneratorCriticOutcome` (with `Judgement`) are plain values with public constructors, so a test double of a node or a harness can build one; `structured_reply` gives the first structured block of a model response, or nothing.

## Re-exports

`Fields`, `InstanceId`, `Reason`, `Cause`, `CancelView`, `Event`, `InputReader` and `Node` of `br-graph-contract`, and `StreamSink` of `br-llm-clients-core`. Types of `br-llm-messages` are named by the signatures too: pin that library, `br-llm-clients-core` and `br-graph-contract` on the same git URL and tag as this crate, or the types split in two copies.

## Provided values

| Item | Rule it implements |
|---|---|
| `Tool`, `ToolCode`, `ToolContent`, `ToolBlock`, `ToolSet` | a tool gets the arguments, the cancel view and the result stream; it gives blocks without identities and an error flag, or a fault; `into_result` gives each block its identity; `ToolSet` refuses a name twice |
| `WebAccess`, `Site`, `AgentLabel`, `ThreadRef`, `MailId`, `EntryItem` | validated values: a site is non-blank, a label non-empty, a thread reference and a mail id a UUIDv7; an empty whitelist is valid and reaches no site |
| `NewMail`, `MailItem`, `MailDocument`, `IncomingBatch`, `IncomingContext`, `AiTurnRef`, `CriticRequest` | plain values with public fields, not validated at construction; `NewMail::refusal` is the check of a mail (no recipient in `to`, a recipient listed twice in `to` and `cc`) that the mail tools and `MemoryIncoming::deliver_mail` make |

`AgenticNode<F>` is a `Node<F>` that tells `has_incoming()`; its `kind()` is expected to be `AGENTIC` (not enforced).

## In-memory ports (`memory`)

For tests, not for production. One `Record` is shared by every in-memory port and keeps every call, in order, with what the streams took: build every port from the same `&Record` and read `record.calls()`.

- `MemoryThreadAccess::new(&record, thread, conversation)` starts from the conversation the test gives, and `MemoryThreadAccess::empty(&record, thread, identity)` from an empty conversation with the identity the test gives; it applies each addition with the `apply_*` functions (a refused addition is a fault), and `read` gives back the completed additions only, never an open object. `conversation()` and `is_open()` let a test look.
- `MemoryIncoming` gives what the test sets with `deliver_entry` and `deliver_mail`, and tells `is_read`; it keeps the mails it was given (`mail`). It has no link to the thread access.
- `MemoryAgentPort` keeps one identity for each label as long as it lives (a new port models a retried job; it mints one for a label the test did not name); `with_agent` sets an identity. `FixedThreadPort` and `FixedToolsPort` give what the test sets; `FixedToolsPort` records the `RunContext` it receives.

## Why

| Thing | Why it is the way it is |
|---|---|
| the `apply_*` functions are here | the in-memory thread access must refuse an invalid write as a real one does; one copy of the rules, not two |
| `AiTurnRef` names agent and author on every call | a thread holds several agents; the call that creates an AI turn must give them, and the thread access needs no other state |
| `IncomingPort` has no `wait`, `send` or `documents` | the node reads at each boundary and never waits; sending a mail and reading its documents belong to the mail tools of the harness |
| the entry id of a mail is the mail id | an agent that gets the same mail twice finds the entry held and does not store it twice |
| `ToolContent` blocks have no identity | the instance gives every identity when it builds the `RuntimeToolResult` |
| a send to an ended stream is not recorded | the in-memory streams take nothing after the object ended or was opened again |
| no `restart()` on `AgenticNode` | what a restart does to the state is said by each node library |
| no workspace port, no file system mode | the file system and the sandbox are the harness's concern |

## Not enforced

Left to the implementer, as the intent states: a tool is safe to call from several runs; a tool that starts background work is given only to a node that has an incoming port; the incoming port lists every unread item of the agent; the agent port gives another identity for another label; one instance runs at a time for one agent on one thread; the implementer drops an open object when its instance ended and ignores what is sent to an ended stream; an implementer that keeps open objects accepts opening one that is open and discarding one that is not; the thread, tools, prompt, output schema, author, outcome and critic ports give what belongs to the run of the snapshot; `AgenticNode::kind()` returns `AGENTIC`.

## Install

Not on crates.io. Pin the git tag with the matching `version` beside it, and pin `br-graph-contract`, `br-llm-messages` and `br-llm-clients-core` on the same git URLs and tags:

```toml
br-agentic-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-contract", tag = "br-agentic-contract/v0.3.0", version = "0.3.0" }
```
