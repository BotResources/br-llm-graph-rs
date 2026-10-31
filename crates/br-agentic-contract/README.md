# br-agentic-contract

What every agentic node has in common, in hexagonal terms.

- A **port** is a trait of this crate. The runner gives the ports to a node when it builds it.
- An **adapter** is what a runner library (for example `br-runner-threads`, `br-runner-jobs`) implements for a port.
- The node libraries use the ports and the machinery of this crate; they never import an adapter.

The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

## Start here

To write an adapter:

- Entry point: pick the port, implement its trait, return `PortError::Refused` only where the table of [The ports](#the-ports) says the port refuses, and `PortError::Fault` for anything else.
- Skeleton: `tests/contract/adapter_example.rs` is a compile-checked skeleton of four ports (its inbox lists nothing, never wakes and marks nothing; `src/memory/inbox.rs` is a working inbox). Two of its ports are shown below as doctested snippets: the agent port (a process map of identities: a real runner saves the identity with its first instance, so that a restart gives the same identity back) and the workspace port.
- Imports: the types of `br-graph-contract` that a port signature names are re-exported by this crate, with `Event`, `InputReader` and `Node`; the agent port and the inbox need no `br-llm-clients-core` import. The thread access names `StreamSink` (re-exported) and the tools port names `Tool`, which `Tool::new` builds from a `ToolSpec` of `br-llm-clients-core` (not re-exported).
- Thread access: the hardest port. `tests/contract/forgetful.rs` is a compile-checked thread access that accepts every call and keeps nothing (`tests/contract/session/independence.rs` proves that the instance gives the same result with it as with one that keeps everything); `src/memory/thread.rs` and `src/memory/streams.rs` show one that keeps what it receives. `ThreadPort` gives the reference of the thread; `ThreadAccess` reads the conversation and receives the additions.
- The duties that no type enforces are in [Not enforced](#not-enforced).

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

This is a workspace port that keeps the files in memory.

```rust
use std::sync::Mutex;

use br_agentic_contract::{Fields, PortError, PortFuture, WorkspaceFile, WorkspacePort};

struct KeptFiles {
    files: Mutex<Vec<WorkspaceFile>>,
}

impl<F: Fields> WorkspacePort<F> for KeptFiles {
    fn load<'a>(
        &'a self,
        _snapshot: &'a F,
    ) -> PortFuture<'a, Result<Vec<WorkspaceFile>, PortError>> {
        Box::pin(async move {
            let files = self.files.lock().map_err(|_| PortError::fault("poisoned"))?;
            Ok(files.clone())
        })
    }

    fn save<'a>(
        &'a self,
        _snapshot: &'a F,
        files: Vec<WorkspaceFile>,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            *self.files.lock().map_err(|_| PortError::fault("poisoned"))? = files;
            Ok(())
        })
    }
}
```

## The ports

All ports are public, object-safe (`dyn ThreadPort<F>`, `dyn InboxPort`, ... compile), `Send + Sync`, and asynchronous: a method returns a `PortFuture`, a boxed `Send` future that this crate owns, so the signatures name no future type of `br-llm-clients-core`. The instance never retries a call to a port. A failure of a port other than the inbox is a fault of the instance; no error of the inbox is.

| Port | What it does | Refuses a request? |
|---|---|---|
| `ThreadPort<F>` | state snapshot to a `ThreadRef` (a UUIDv7) | never |
| `AgentPort<F>` | snapshot, instance id and `AgentLabel` to an `AgentId` | never |
| `ToolsPort<F>` | snapshot and instance id to the `Tool`s of the instance | never |
| `WorkspacePort<F>` | `load` the files at the start, `save` them at the end | never |
| `ThreadAccess` | `read` the conversation once; every other operation receives a complete addition: `add_input_turn`, `add_entry`, `close_turn`, `open_*` (gives a stream), `complete_*`, `discard_*` | never (a refusal is a fault) |
| `InboxPort` | `items` (batches and cursor), `wait`, `mark_read`, `send`, `documents` | `send` (no recipient in `to`, a recipient twice) and `documents` (agent not a recipient in `to`) |

The write path of an adapter (`add_*`, `open_*` (it gives the stream), `close_turn`, `complete_*`, `discard_*`, `WorkspacePort::save`, `mark_read`, `send`, the stream sends) returns when the adapter has accepted the data; its result carries only accepted or a fault, and a refusal only where the table above says so. At the start the instance awaits the thread port, the agent port, the tools port, the `load` of the workspace port (by file system mode) and the `read` of the thread access; at the end it awaits the `save`; it calls `items`, `wait` and `documents` of the inbox when the node library chooses, and an error of any inbox call is not a fault of the instance.

`PortError` is the one error of the ports: `Refused(Reason)` (a reason as text, an expected error) or `Fault(Cause)` (an opaque cause). No error of the inbox is a fault of the instance: the mail tools of the node library give it to the agent as a tool result with the error flag set, and after a failed read, wait or mark as read the instance goes on with the items it holds. A node library maps any inbox error, refusal or fault, to a tool result with the error flag set (`ToolContent::error_text`); the contract gives no helper for it. The `Error` of `br-llm-messages` is part of the public API, as the types of that library are: `SessionError::Rules`, `MailEntryBuilder`, `ToolBlock::text`, `ToolContent::error_text`, `ToolContent::into_result` and `MailId::from_uuid` give it as it is. `SessionError::Port` has no `From<PortError>`: it is built only for the thread access, and `Queue::read` and `mail_documents` give a bare `PortError`, so `?` never turns an inbox error into a fault. `Boundary`, `BoundaryError`, `SessionError`, `ToolBlock` and `memory::Call` are `#[non_exhaustive]`: match them with a wildcard arm; `Boundary::default()` is an empty report. A `Queue::read` that gets the same cursor twice in a row gives a `PortError::Fault` of that read (an adapter bug; the instance is not faulted by it alone, and the items already read stay in the queue); it does not detect a cursor that comes back later, nor new cursors on empty pages. `SessionError::WrongTurn` means that a call names a turn that is not the AI turn of that agent and author (an input turn, or an AI turn of another agent or author), or closes an AI turn that does not exist. `SessionError::WrongMailEntry` means that a `MailEntryBuilder` gave an entry that is not the entry of its mail (see `store_at_boundary` below).

## Re-exports

`Fields`, `InstanceId`, `Reason`, `Cause`, `CancelView`, `Event`, `InputReader` and `Node` of `br-graph-contract`, and `StreamSink` of `br-llm-clients-core`. Types of `br-llm-messages` (`AgentId`, `Conversation`, `EntryId`, ...) and `ToolSpec` of `br-llm-clients-core` are named by the signatures too: pin those libraries on the same git URL and tag as this crate, or the types split in two copies.

## What the crate provides to a node library

| Item | Rule it implements |
|---|---|
| `Session` | reads the conversation once (`Session::start`) and keeps the copy of the instance; `add_input_turn`, `add_entry`, `close_turn`, `complete_response` and `complete_result` apply the addition to the copy under the rules of `br-llm-messages` first and hand it to the thread access only when accepted; `open_response` and `open_result` give the stream and change nothing in the copy; a refused completion discards the open object and is an error (a failure of that discard is not reported, and the object is no longer tracked); tracks open objects; after a `SessionError::Port` from `add_input_turn`, `add_entry`, `close_turn`, `complete_response` or `complete_result`, or when the future of one of these calls is dropped before it ends, the copy holds an addition that the thread access did not take (a `SessionError::Port` from `open_response` or `open_result` leaves the copy as it was), so the instance ends with `failed` and the session must not be used for a boundary again (a boundary on it would skip and mark an item that the thread never received) |
| `Queue`, `store_at_boundary`, `MailEntryBuilder` | the queue of unread items (an id held earlier is skipped; `Queue::read` fills it from the inbox batches); the instance stores each new item of its queue at a boundary: items the copy holds are skipped and marked, the others go into the open AI turn of the agent after its last entry, else into one new input turn with one entry each; the entry that the builder gives for a mail item must have the mail id, and be an agent mail with the sender, the author name and the source conversation of the item for a `to` item, or a runtime notice for a `cc` item, else the boundary gives `SessionError::WrongMailEntry` before it stores or marks anything (the queue is restored); an item is marked as read only after its entry is handed over; an error of a mark stops nothing (the item counts as stored or skipped, the next items go on, `Boundary::unmarked` lists the ids); an error of `Queue::read` leaves in the queue the items it already gave; the node library builds the entry of a mail; after a `BoundaryError` that carries a `SessionError::Port` the queue is restored but the session must not be reused (see `Session`) |
| `mail_documents` | the documents of a mail only when the conversation holds an incoming entry with the mail id; else a refusal, and the inbox is not asked |
| `result`, `failed`, `cancelled` | discard every open object and close the input end before the terminal event; `failed` and `cancelled` ignore an error of a discard; at `result` a failed discard gives `Event::Failed` with the first error of the discards as its cause, instead of `Event::Result` |
| `FileSystemMode::start`, `save_files` | `Always`, `Might` and `Never` and when the load is called; the save of the files before `Result` |
| `Tool`, `ToolCode`, `ToolContent`, `ToolBlock`, `ToolSet` | a tool gets the arguments, the cancel view and the result stream; it gives blocks without identities and an error flag, or a fault; `into_result` gives each block its identity; `ToolSet` refuses a name twice, mail tools included |
| `WebAccess`, `Site`, `AgentLabel`, `ThreadRef`, `MailId`, `InboxEntry` | validated values: a site is non-blank, a label non-empty, a thread reference and a mail id a UUIDv7; an empty whitelist is valid and reaches no site |
| `NewMail`, `MailItem`, `MailDocument`, `WorkspaceFile`, `InboxBatch`, `InboxContext`, `AiTurnRef` | plain values with public fields, not validated at construction; `NewMail::refusal` is the check an inbox adapter makes at `send` (no recipient in `to`, a recipient listed twice in `to` and `cc`); the crate checks no subject, body, attachment name or path |

`AgenticNode<F>` is a `Node<F>` that tells `has_inbox()`; its `kind()` is expected to be `AGENTIC` (not enforced).

## In-memory ports (`memory`)

For tests, not for production. One `Record` is shared by every in-memory port and keeps every call, in order, with what the streams took: build every port from the same `&Record` and read `record.calls()`.

- `MemoryThreadAccess::new(&record, thread, conversation)` starts from the conversation the test gives, and `MemoryThreadAccess::empty(&record, thread, identity)` from an empty conversation with the identity the test gives; applies each addition under the rules of `br-llm-messages` (a refused addition is a fault), `read` gives back the completed additions only, never an open object. `conversation()` and `is_open()` let a test look.
- `MemoryInbox` gives what the test sets with `deliver_entry`, `deliver_mail`, `put_document`; a mail sent with `send` is delivered to its recipients the same way. It has no link to the thread access.
- `MemoryAgentPort` keeps one identity for each label as long as it lives (a new port models a retried job; it mints one for a label the test did not name); `with_agent` sets an identity. `MemoryWorkspace`, `FixedThreadPort` and `FixedToolsPort` give what the test sets.

## Why

| Thing | Why it is the way it is |
|---|---|
| `Session` owns the conversation copy | the intent has one read at the start and "rules first": only a copy of the instance can say a change is refused before the thread access sees it |
| `AiTurnRef` names agent and author on every call | a thread holds several agents; the call that creates an AI turn must give them, and the thread access needs no other state |
| `store_at_boundary` never extends an input turn | the thread access receives complete additions only; items with no open AI turn form a new input turn |
| `MailEntryBuilder` is a parameter | the content of a mail entry and its escaping belong to the node libraries |
| a failed mark is listed in `Boundary::unmarked`, not retried | an unmarked item stays unread in the inbox and the conversation holds it, so the next boundary skips and marks it; the instance never retries a call to a port |
| `ToolContent` blocks have no identity | the instance gives every identity when it builds the `RuntimeToolResult` |
| a send to an ended stream is not recorded | the in-memory streams take nothing after the object ended or was opened again |
| `wait` gives nothing | the caller reads the list; an item a read already gave and nobody marked still ends the wait at once |
| the entry id of a mail is the mail id | two agents that share one thread and receive the same mail: when the copy of the second was read after the first stored the entry, its boundary finds the entry held, skips the item and marks it read, so that agent gets no entry of its own; when the copy was read before, its boundary hands the same entry id again and the thread access refuses it (a fault, nothing marked); which agents share a thread is the runner's choice |
| no `restart()` on `AgenticNode` | what a restart does to the state is said by each node library |

## Not enforced

Left to the implementer, as the intent states: a tool is safe to call from several runs; a tool that starts background work is given only to a node with an inbox; the inbox lists every unread item; the agent port gives another identity for another label; one instance runs at a time for one agent on one thread; the implementer drops an open object when its instance ended and ignores what is sent to an ended stream; an implementer that keeps open objects accepts opening one that is open and discarding one that is not; the inbox sends a mail once for one mail id; the thread, tools and workspace ports give what belongs to the run of the snapshot; the instance reads the inputs it accepted before `Result` (`result` takes the `InputReader` and closes the input end, it does not read the inputs; `InputReader::close` is idempotent); a `MailEntryBuilder` gives an entry with no document in the content (the boundary checks the id, the kind and, for a `to` item, the sender, the author name and the source conversation; the content is the node library's); `store_at_boundary` is not cancel-safe: it takes the items out of the queue and gives them back only when it returns an error, so a dropped future loses the items of that instance not yet processed (the items stored and marked before the drop are in the thread and marked; the others stay unmarked in the inbox); `AgenticNode::kind()` returns `AGENTIC`; after a fault of the thread access the session is not reused for a boundary.

## Install

Not on crates.io. Pin the git tag with the matching `version` beside it, and pin `br-graph-contract`, `br-llm-messages` and `br-llm-clients-core` on the same git URLs and tags:

```toml
br-agentic-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-contract", tag = "br-agentic-contract/v0.2.0", version = "0.2.0" }
```
