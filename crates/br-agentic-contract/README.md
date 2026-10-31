# br-agentic-contract

What every agentic node has in common, in hexagonal terms.

- A **port** is a trait of this crate. The runner gives the ports to a node when it builds it.
- An **adapter** is what a runner library (for example `br-runner-threads`, `br-runner-jobs`) implements for a port.
- The node libraries use the ports and the machinery of this crate; they never import an adapter.

The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

## Start here

To write an adapter: pick the port, implement its trait, return `PortError::Refused` only where the table says the port refuses, and return `PortError::Fault` for anything else. The types of `br-graph-contract` that a port signature names are re-exported by this crate, so an adapter needs no `br-graph-contract` import; the agent port and the inbox need no `br-llm-clients-core` import either. A complete, compile-checked adapter of four ports is `tests/contract/adapter_example.rs`; this is its agent port.

```rust,ignore
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

## The ports

All ports are public, object-safe (`dyn ThreadPort<F>`, `dyn InboxPort`, ... compile), `Send + Sync`, and asynchronous: a method returns a `PortFuture`, a boxed `Send` future that this crate owns, so an adapter needs no `br-llm-clients-core` import except for the response stream of the thread access. The instance never retries a call to a port. A failure of a port other than the inbox is a fault of the instance; no error of the inbox is.

| Port | What it does | Refuses a request? |
|---|---|---|
| `ThreadPort<F>` | state snapshot to a `ThreadRef` (a UUIDv7) | never |
| `AgentPort<F>` | snapshot, instance id and `AgentLabel` to an `AgentId` | never |
| `ToolsPort<F>` | snapshot and instance id to the `Tool`s of the instance | never |
| `WorkspacePort<F>` | `load` the files at the start, `save` them at the end | never |
| `ThreadAccess` | `read` the conversation once; every other operation receives a complete addition: `add_input_turn`, `add_entry`, `close_turn`, `open_*` (gives a stream), `complete_*`, `discard_*` | never (a refusal is a fault) |
| `InboxPort` | `items` (batches and cursor), `wait`, `mark_read`, `send`, `documents` | `send` (no recipient in `to`, a recipient twice) and `documents` (agent not a recipient in `to`) |

The write path of an adapter (`add_*`, `close_turn`, `complete_*`, `discard_*`, `mark_read`, `send`, the stream sends) returns when the adapter has accepted the data; its result carries only accepted, refused or fault. The instance waits on the `read` of the start; it calls `items`, `wait` and `documents` of the inbox when the node library chooses, and an error of any inbox call is not a fault of the instance.

`PortError` is the one error of the ports: `Refused(Reason)` (a reason as text, an expected error) or `Fault(Cause)` (an opaque cause). No error of the inbox is a fault of the instance: the mail tools of the node library give it to the agent as a tool result with the error flag set, and after a failed read, wait or mark as read the instance goes on with the items it holds. `ToolContent::from_port_error` is for the ports other than the inbox: it turns a refusal into a tool result with the error flag set and a fault into a `ToolFault`.

## Re-exports

`Fields`, `InstanceId`, `Reason`, `Cause`, `CancelView`, `Event`, `InputReader` and `Node` of `br-graph-contract`. Types of `br-llm-messages` (`AgentId`, `Conversation`, `EntryId`, ...) and the `StreamSink` of `br-llm-clients-core` are named by the signatures too: pin those libraries on the same git URL and tag as this crate, or the types split in two copies.

## What the crate provides to a node library

| Item | Rule it implements |
|---|---|
| `Session` | reads the conversation once (`Session::start`) and keeps the copy of the instance; every addition is applied to the copy under the rules of `br-llm-messages` first and handed to the thread access only when accepted; a refused completion discards the open object and is an error; tracks open objects |
| `Queue`, `store_at_boundary`, `MailEntryBuilder` | the queue of unread items (an id held earlier is skipped; `Queue::read` fills it from the inbox batches); the instance stores each new item of its queue at a boundary: items the copy holds are skipped and marked, the others go into the open AI turn of the agent after its last entry, else into one new input turn with one entry each; an item is marked as read only after its entry is handed over; an error of a mark stops nothing (the item counts as stored or skipped, the next items go on, `Boundary::unmarked` lists the ids); an error of `Queue::read` leaves in the queue the items it already gave; the node library builds the entry of a mail |
| `mail_documents` | the documents of a mail only when the conversation holds an incoming entry with the mail id; else a refusal, and the inbox is not asked |
| `result`, `failed`, `cancelled` | discard every open object (errors of the discards are ignored) and close the input end before the terminal event |
| `FileSystemMode::start`, `save_files` | `Always`, `Might` and `Never` and when the load is called; the save of the files before `Result` |
| `Tool`, `ToolCode`, `ToolContent`, `ToolBlock`, `ToolSet` | a tool gets the arguments, the cancel view and the result stream; it gives blocks without identities and an error flag, or a fault; `into_result` gives each block its identity; `ToolSet` refuses a name twice, mail tools included |
| `WebAccess`, `Site`, `AgentLabel`, `ThreadRef`, `MailId`, `NewMail`, `MailItem`, `InboxEntry` | validated values: a site is non-blank, a label non-empty, a thread reference and a mail id a UUIDv7; an empty whitelist is valid and reaches no site |

`AgenticNode<F>` is a `Node<F>` of kind `AGENTIC` that tells `has_inbox()`.

## In-memory ports (`memory`)

For tests, not for production. One `Record` is shared by every in-memory port and keeps every call, in order, with what the streams took: build every port from the same `&Record` and read `record.calls()`.

- `MemoryThreadAccess::new(&record, thread, conversation)` starts from the conversation the test gives (`Conversation::new(identity)` for an empty one), applies each addition under the rules of `br-llm-messages` (a refused addition is a fault), `read` gives back the completed additions only, never an open object. `conversation()` and `is_open()` let a test look.
- `MemoryInbox` gives what the test sets with `deliver_entry`, `deliver_mail`, `put_document`; a mail sent with `send` is delivered to its recipients the same way. It has no link to the thread access.
- `MemoryAgentPort` keeps one identity for each label as long as it lives (a new port models a retried job); `with_agent` sets an identity. `MemoryWorkspace`, `FixedThreadPort` and `FixedToolsPort` give what the test sets.

## Why

| Thing | Why it is the way it is |
|---|---|
| `Session` owns the conversation copy | the intent has one read at the start and "rules first": only a copy of the instance can say a change is refused before the thread access sees it |
| `AiTurnRef` names agent and author on every call | a thread holds several agents; the call that creates an AI turn must give them, and the thread access needs no other state |
| `store_at_boundary` never extends an input turn | the thread access receives complete additions only; items with no open AI turn form a new input turn |
| `MailEntryBuilder` is a parameter | the content of a mail entry and its escaping belong to the node libraries |
| `from_port_error` replaces a blank refusal reason by a fixed text | a refusal stays an expected error even if the adapter gave no text |
| a failed mark is listed in `Boundary::unmarked`, not retried | an unmarked item stays unread in the inbox and the conversation holds it, so the next boundary skips and marks it; the instance never retries a call to a port |
| `ToolContent` blocks have no identity | the instance gives every identity when it builds the `RuntimeToolResult` |
| a send to an ended stream is not recorded | the in-memory streams take nothing after the object ended or was opened again |
| `wait` gives nothing | the caller reads the list; an item a read already gave and nobody marked still ends the wait at once |
| no `restart()` on `AgenticNode` | what a restart does to the state is said by each node library |

## Not enforced

Left to the implementer, as the intent states: a tool is safe to call from several runs; a tool that starts background work is given only to a node with an inbox; the inbox lists every unread item; the agent port gives another identity for another label; one instance runs at a time for one agent on one thread; the implementer drops an open object when its instance ended and ignores what is sent to an ended stream; an implementer that keeps open objects accepts opening one that is open and discarding one that is not; the inbox sends a mail once for one mail id; the thread, tools and workspace ports give what belongs to the run of the snapshot.

## Install

Not on crates.io. Pin the git tag with the matching `version` beside it, and pin `br-graph-contract`, `br-llm-messages` and `br-llm-clients-core` on the same git URLs and tags:

```toml
br-agentic-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-contract", tag = "br-agentic-contract/v0.2.0", version = "0.2.0" }
```
