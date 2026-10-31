# Changelog

All notable changes to `br-agentic-contract` are documented here. The git tag
`br-agentic-contract/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.2.0 - 2026-10-31

Rework of the thread access, the ports, the in-memory ports and the queue rules to the sealed intent of minor 0.2. Breaking: the 0.1 API of the thread access, the inbox, the agent port and the tool content does not compile against 0.2.

### Breaking

- One error type for the ports, `PortError` (`Refused(Reason)`, `Fault(Cause)`), replaces `PortFault`, `ThreadError` and `InboxError`. Only the inbox refuses (`send`, `documents`); a refusal by the thread access is a fault of the instance.
- All port methods return `PortFuture`, a boxed `Send` future owned by this crate, instead of the `BoxFuture` of `br-llm-clients-core` (the same type behind the alias). The agent port and the inbox need no clients-core import.
- `ThreadAccess` is rebuilt: one `read(thread)` gives the whole `Conversation`, used once at the start; the other operations receive complete additions (`add_input_turn(InputTurn)`, `add_entry(AiTurnRef, TurnEntry)`, `close_turn`, `open_response`/`open_result` giving a stream, `complete_*`, `discard_*`) and answer accepted, refused or fault. Removed: `load` of a `ProjectedThread`, the agent parameter, `Incoming`, `WrittenEntry`, `TurnTarget`, `Stored`, `Opened` and `Completed` with their `Held` answers. `open_result` no longer takes the tool call.
- `AgentPort` gives an `AgentId`; the `Agent` type and the optional author are removed (the author is a build value of the node or a port of the node library).
- `MailItem::To` and `NewMail` carry the author name of the sender and the source conversation; `InboxPort::mark_read` takes the `EntryId` of any item (mail, human message, runtime notification, runtime notice); `InboxPort::wait` gives nothing; `documents` refuses with a reason when the agent is not a recipient in `to`, for an unknown mail too.
- `ThreadRef::new` is replaced by `ThreadRef::from_uuid`, which refuses a UUID that is not a v7. `AgentLabel` refuses only the empty text. `InboxEntry::new` returns `InvalidInboxEntry`.
- `ToolContent` holds `ToolBlock`s (text, image, structured) without identity; `into_result` gives each block its identity; `ToolContent::error_text` takes no block id. `ToolCode` and `ResultStream` use `PortFuture`.
- `Session` holds the conversation copy: `Session::start` reads once; every addition is applied to the copy under the rules of `br-llm-messages` before it is handed over; a refused completion discards the open object. Errors are `SessionError`.
- `store_at_boundary` takes the session, a `MailEntryBuilder` (the node library builds the entry of a mail) and the id of a new input turn: items the copy holds are skipped and marked; the others go into the open AI turn of the agent after its last entry, else into one new input turn with one entry each; every kind of item is marked as read after its entry is handed over. `BoundaryError` carries a `SessionError`; `BoundaryFault` is removed.
- `AgenticNode::restart` and `Restart` are removed.
- The in-memory ports are rebuilt (`MemoryHub` is removed): they take one shared `Record` and keep one ordered record of every call, with what the streams take; the thread access keeps the completed additions and starts from the conversation of the test; there is no link between the in-memory inbox and the in-memory thread access; `MemoryAgentPort` takes its identities from the test (it mints a UUIDv7 for a label the test did not name); `FixedThreadPort`, `FixedToolsPort` and `MemoryWorkspace` take the record.
- `Boundary`, `BoundaryError`, `SessionError`, `ToolBlock` and `memory::Call` are `#[non_exhaustive]`; `SessionError` has no `From<PortError>` (an inbox error never becomes a fault through `?`); `BoundaryError` keeps its fault as the `source`.

### Added

- `PortFuture`, `PortError`, `AiTurnRef`, `ToolBlock`, `SessionError`, `MailEntryBuilder`, `InvalidThreadRef`, `InvalidInboxEntry`.
- `mail_documents` (documents only for a mail whose entry the conversation holds as an incoming entry), `Queue::read` (fills the queue from the batches of the inbox; it gives a `PortError::Fault` when the inbox gives the same cursor twice in a row, and keeps the items it already read), `Boundary::unmarked`, `Session::holds`, `Session::holds_incoming`, `Session::open_ai_turn`.
- Re-exports of the types of `br-graph-contract` that the ports name: `Fields`, `InstanceId`, `Reason`, `Cause`, `CancelView`, `Event`, `InputReader`, `Node`, and of the `StreamSink` of `br-llm-clients-core`.
- `memory::Record` and `memory::Call`, `memory::SentMail`, `MemoryThreadAccess::empty` (an empty conversation with the identity the test gives).
- `SessionError::WrongTurn` (a call names a turn that is not the AI turn of that agent and author, or closes an AI turn that does not exist) and `SessionError::WrongMailEntry` (the `MailEntryBuilder` gave an entry that is not the entry of its mail: another id than the mail id, or not an agent mail for a `to` item or not a runtime notice for a `cc` item, or an agent mail whose sender, author name or source conversation is not that of the item; `store_at_boundary` then stores and marks nothing and restores the queue).
- A crate README (the contract of the crate).

### Changed

- `WebAccess::reaches_nothing` is also true for an empty whitelist.
- `result` gives `Event::Failed` with the first error of the discards as its cause when a discard of an open object fails (in 0.1 it ignored the error); `failed` and `cancelled` still ignore it.
- `ToolContent::from_port_error` of the earlier 0.2.0 work in progress is not shipped, and nothing replaces it: this crate holds no mail tools, a node library maps any inbox error to a tool result with `ToolContent::error_text`.
- An inbox error is never a fault of the instance: mark errors no longer stop `store_at_boundary` (the item counts as stored or skipped and `Boundary::unmarked` lists the ids).

## 0.1.0 - 2026-10-29

First release: the contract of the agentic nodes. It builds on `br-graph-contract`, `br-llm-messages` and `br-llm-clients-core`, and on no node library.

### Added

- `AgenticNode<F>` (a `Node<F>` that tells `has_inbox()` and declares `restart()` as `Restart::{Resets, Keeps, NoCounters}`) and `AGENTIC`, the kind of an agentic node.
- The ports a runner gives a node, all asynchronous (`BoxFuture`), none of them retried: `ThreadPort` (state snapshot to `ThreadRef`), `AgentPort` (snapshot, instance id and `AgentLabel` to an `Agent`: an `AgentId` and an optional `Author` when the runner supplies the name at run time), `ToolsPort`, `WorkspacePort` (`load`, `save`), `ThreadAccess` and `InboxPort`. A failure of a port is a `PortFault` with an opaque cause. The state snapshot is the fields `F` of the first input.
- `ThreadAccess`: `load` the `ProjectedThread` of one agent; `add_input_turn`, `add_incoming` (to a `TurnTarget`: an input turn, or an AI turn with its author, created by its first entry), `close_turn`; `open_response` / `complete_response` / `discard_response` and `open_result` / `complete_result` / `discard_result`. An `Incoming` addition is a `MailItem` (the thread access builds the entry: an agent mail for a recipient in `to`, a runtime notice for one in `cc`, with the mail id as the entry id and no attachment in the content), an `InboxEntry` (a human message, a runtime notification or a runtime notice, as the inbox gave it) or a `WrittenEntry` (an agent mail the instance writes, with the author name it gives). An addition whose identity the thread holds already answers `Stored::Held` and changes nothing, before the rules of `br-llm-messages` apply; a new addition those rules refuse is `ThreadError::Refused` with a `Reason`; any other failure is `ThreadError::Fault`. `open_*` answers `Opened::Stream` or `Opened::Held(object)`; `complete_*` answers `Completed::Added` or `Completed::Held(object)`. `ResponseStream` is a `StreamSink` of `br-llm-clients-core`; `ResultStream` takes fragments of text; neither has an error.
- `InboxPort`: `items` (oldest first, bounded batches, an opaque `Cursor` for the next batch, `None` when the list has no more items), `wait` (resolves with the next item for the agent that the caller has not been given yet, by a read or by an earlier `wait`), `mark_read` (the only change to the state of a mail), `send` (a `NewMail`, whose sender is the agent of the `InboxContext`; refused as `InboxError::MailRefused` with a `Reason` when it has no recipient in `to` or lists a recipient twice; `NewMail::refusal` is that check; sending an id sent already changes nothing) and `documents`. `InboxItem` is a `MailItem` or an `InboxEntry`; `MailItem::To` carries the mail id, sender, recipients, subject, body and attachment names, `MailItem::Cc` the mail id and subject.
- `Tool` (a `ToolSpec` with a `ToolCode` that gets the arguments, a `CancelView` and a `ResultStream`), `ToolContent` (whole content and error flag; an expected error is content with the error flag, not a fault), `ToolFault` (opaque cause) and `ToolSet`, which refuses a name that appears twice among the tools and the mail tools (`DuplicateTool`).
- `WebAccess` (`Full`, `Whitelist(Vec<Site>)`, `None`), `FileSystemMode` (`Always`, `Might`, `Never`) with `FileSystemMode::start` (loads the files per mode: `Never` does not call the load; `Might` has a file system only when the load gives at least one file) and `save_files`.
- `BoundaryError` carries the `Boundary` done before the failure (`fault` and `done`); `BoundaryFault` is the cause. `store_at_boundary` needs a new turn id on every call: a turn id the thread holds, with an item that is not held, is refused.
- The rules of an instance, as functions for the node libraries: `Queue` and `store_at_boundary` (stores the queued items in arrival order into the open AI turn, else the input turn that ends the thread (an input turn that shows the AI turn of another agent is not one: a new input turn is made), else one new input turn; skips and marks an item the projected thread holds; marks a mail as read only after its entry is stored; leaves unstored items in the queue), `Session` (opens, completes and discards objects, and discards every object still open), and `result`, `failed` and `cancelled`, which close the input end and discard the open objects, ignoring the errors of the discards.
- `memory`: `MemoryHub` with `MemoryThreadAccess`, `MemoryInbox`, `MemoryAgentPort`, `MemoryWorkspace`, `FixedThreadPort` and `FixedToolsPort`, for tests. The hub keeps one `Conversation` for each thread and projects it for an agent; an addition is stored whole or not at all; an open object is never in the projection; a send to a stream that has ended is ignored. The hub knows the author name of an agent only when the test gives it (`set_author` or `MemoryAgentPort::with_author`); a mail of an agent without a name is a fault.
