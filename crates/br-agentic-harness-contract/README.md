# br-agentic-harness-contract

What the harness of an agentic node asks from a runner, in hexagonal terms. The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

- The harness is an optional layer between an agentic node and the runner that runs it. The node keeps seeing the node ports of `br-agentic-contract`; the harness fills them on one side and asks the ports of this crate from the runner on the other side.
- This crate is a contract library: port traits, the plain types of their signatures, the errors that cross them, and in-memory ports for tests. It holds no code that does a job through a port; the harness logic and the mail tools are in `br-agentic-harness`.
- Importers: the harness, and the adapter libraries of a runner (history, mail, sandbox, skills). A runner that needs no harness does not import it.

## The six ports

All ports are public, object-safe, `Send + Sync` and asynchronous (`PortFuture` of `br-agentic-contract`). They share its error, `PortError`: `Refused(Reason)` for an expected error, `Fault(Cause)` for anything else. A port can serve several instances at once and never decides policy. It is usable as a trait object.

| Port | Defined here | What it does |
|---|---|---|
| `HarnessHistory` | yes | every operation of `ThreadAccess`, plus `stored(thread, entry)` |
| `PromptPort`, `ToolsPort` | no, re-exported | the node ports of `br-agentic-contract`; one trait, one type |
| `MailPort` | yes | `items`, `mark_read`, `send`, `documents` for an `IncomingContext` (instance, thread, agent) |
| `SkillsPort` | yes | `list` the skills of an instance; `read` a file of a skill |
| `SandboxPort` | yes | `open`, `run`, `read`, `write`, `list`, `kill`, `save`, `release` on the box of one run |

`ToolsPort::tools` takes a `&RunContext`: instance, thread, agent (the main agent), author (of the main seat) and conversation id (the source of a new mail; it is not the thread). `RunContext` is `#[non_exhaustive]` and made by `RunContext::new`; the gate of `get_mail_files` is not in it.

## Stored signal

`HarnessHistory` is a subtrait of `ThreadAccess`, not a supertrait of it (a supertrait would make `br-agentic-contract` depend on this crate). An adapter of the history port serves as the node history port too, and `&dyn HarnessHistory` coerces to `&dyn ThreadAccess`.

`stored(thread, entry)` answers `StoredAnswer::Stored` (the thread holds the entry) or `StoredAnswer::Dropped` (this item of the entry will not be stored; a later Run can store the entry again). An entry the adapter has no record of, a thread it has no slot for, and a closed change channel are a `PortError`, never `Stored`. The future ends when the answer is known; dropping it loses nothing.

The harness marks a mail read only after `Stored` for the entry of that mail. After `Dropped`, a fault or a refusal it does not mark: the mail stays unread. A crash between the store and the mark gives a second delivery, never a lost mail.

## Mail port

| Operation | Rule |
|---|---|
| `items` | the unread items of the agent, oldest first, in batches of at most `limit`, with a `Cursor`; a read item is not listed; reading again is allowed and the list is the truth |
| `mark_read` | returns when the implementer accepted the mark; an item already read, or an id the inbox does not hold for the agent, changes nothing and is no error |
| `send` | the sender is the agent of the context; no recipient in `to`, or a recipient twice in `to` and `cc`, is a refusal; a mail id sent already is accepted and changes nothing |
| `documents` | the documents of a mail of which the agent is a recipient in `to`; no attachment gives none; any other mail is a refusal |

There is no operation to wait for a new item.

## Skills port and sandbox port

- `SkillsPort::list(instance)` gives a `SkillSummary` (`SkillName`, description) per skill; `read(instance, skill, path)` gives the bytes of a file; an unknown skill or path is a refusal.
- `SandboxPort::open(&OpenRequest)` gives an `Opened` (the `BoxId` and how it started); the `WebAccess` of the node travels in the request, unchanged, and `None` means no setting, hence no network. The sandbox enforces it, the harness never does.
- `run(box, command, mode)` gives `RunOutcome::Finished(CommandResult)` for `RunMode::Foreground` and `RunOutcome::Started(ProcessId)` at once for `RunMode::Background`. `read` of a missing file is a refusal. `kill` of an ended process changes nothing. `save` keeps the work folder and gives a `SaveReceipt`; `release` ends the use of the box. How a provider builds, keeps or deletes a box is not the concern of this crate.
- `FilePath` is a relative path that stays in the folder (no absolute path, no `..`, no backslash, no NUL). `SkillName` and `Command` are non-blank.
- The sandbox and skills ports are optional for a runner: with none, the harness does mail and history only.

### Sandbox port 0.2: the open request, the start, the save

`SandboxPort::open(&OpenRequest) -> Opened` and `save(box) -> SaveReceipt` are the two calls that changed from 0.1; `run`, `read`, `write`, `list`, `kill` and `release` are as in 0.1.

| Type | What it holds |
|---|---|
| `OpenRequest` | `instance`, `web_access` (`Option<WebAccess>`, passed unchanged, `None` means no network), `turn` (`TurnRef`), `seed` (`Seed`). `#[non_exhaustive]`, made by `OpenRequest::new`; it names no image |
| `TurnRef` | `thread` (`ThreadRef`), `agent` (`AgentId`), `turn` (`TurnId`), `parent` (`Option<TurnId>`). `#[non_exhaustive]`, made by `TurnRef::new`. `AgentId` and `TurnId` are the ids of `br-llm-messages`, re-exported |
| `Seed` | entries and a `ContentHash`. `Seed::new(entries)` refuses two entries with the same path (`InvalidSeed::DuplicatePath`); `Seed::empty()` is valid and a box opened with it holds no seed file. Entries are kept sorted by path. `entries()` gives the entries in that order, `content_hash()` gives the `ContentHash`, and `is_empty()` is true when the seed has no entry |
| `SeedEntry` | `path`, `mode`, `kind`: `SeedKind::File(bytes)`, `SeedKind::Directory`, `SeedKind::Symlink(target)`. `path` and `target` are `FilePath`: relative, no `..`, no backslash, no NUL. `SeedEntry::file(path, mode, bytes)`, `SeedEntry::directory(path, mode)` and `SeedEntry::symlink(path, mode, target)` make one entry of each kind |
| `ContentHash` | SHA-256 over the entries only: sorted by path, each as length-prefixed path, mode, kind tag and payload, after a fixed domain line and the entry count. The order of the entries does not change it; a path, a mode, a kind, a target or one byte does. The value is frozen for the minor (two vectors are pinned by tests). `as_bytes()` gives the 32 bytes; `to_hex()` and `Display` give the same 64 lowercase hex characters |
| `Opened` | `box_id` and `start`. `#[non_exhaustive]`, made by `Opened::new` |
| `StartKind` | `Continued`, `Restored { turn }`, `Fresh(FreshReason)` |
| `FreshReason` | five different values: `NoSaveEver`, `SaveExpired { expired_at: Option<SystemTime> }`, `SaveFailed { turn }`, `RestoreFailed { turn }`, `StoreUnavailable` |
| `SaveReceipt` | `turn`, `made_at` (`SystemTime`) and `bytes`, the raw size of the saved work folder before any compression. `#[non_exhaustive]`, made by `SaveReceipt::new` |

- A save is named by thread, agent and turn id, keeps the work folder and never the seed. The seed is not part of the work folder; every start puts the seed of its own request in the box.
- The start uses the save of the parent turn, of the same thread and the same agent, and no other: not a save of another thread, another agent or another turn, also when newer. A request with no parent turn is `Fresh(NoSaveEver)` and no save is read. Going back to an earlier parent uses the work folder of that turn.
- `Continued`: the box is the box of the parent turn, kept after its save; its work folder is that of the save. `Restored`: the work folder is that of the save of the parent turn. `Fresh`: the box holds the seed and nothing else, never a part of a save. A box that was not saved is never continued, and an expired save is never continued, also when its box is alive.
- A save that is missing, expired, failed or unreadable never makes `open` fail: it gives `Opened` with a `Fresh` start and the reason. The runner, not the port, writes the text that tells the agent what was lost.
- A save of a thread, an agent and a turn that already has one is refused and the first save stays. `AlreadySaved::refusal()` makes this refusal, and `AlreadySaved::is(&error)` tells it from every other refusal. `PortError` carries a refusal as a text only, so the refusal of this crate has the fixed text `AlreadySaved::REASON`, and the test compares it.
- `open` fails only with the refusals and faults of `PortError` and with two named faults, `OpenFault::ImageUnavailable` (no box was ready in the time the provider gives to open; the runner can try again) and `OpenFault::ImageFailing` (the boxes of the provider do not start; the operator must act). They travel as the cause of `PortError::Fault`, never as a refusal; `OpenFault::of(&error)` answers `Some(ImageUnavailable)`, `Some(ImageFailing)` or `None`. A shortage of capacity has no value of its own: it is `ImageUnavailable`.
- If the future of `open` is dropped before it ends, the provider releases everything the call claimed, and no later `open` of the same instance is blocked or refused because of it. This binds every implementation of the port.

```
use br_agentic_harness_contract::{InstanceId, OpenRequest, Seed, TurnRef};

fn by_constructor(instance: InstanceId, turn: TurnRef) -> OpenRequest {
    OpenRequest::new(instance, None, turn, Seed::empty())
}
```

```compile_fail,E0639
use br_agentic_harness_contract::{InstanceId, OpenRequest, Seed, TurnRef};

fn by_fields(instance: InstanceId, turn: TurnRef) -> OpenRequest {
    OpenRequest {
        instance,
        web_access: None,
        turn,
        seed: Seed::empty(),
    }
}
```

## Run lifecycle

`RunLifecycle` is not a port of the runner: the runner calls it towards the harness. `open(instance, turn, seed)` starts the run and gives the `StartKind` of the box: with a sandbox, the harness opens it with an `OpenRequest` made of the instance, its web access setting, the turn reference and the seed, each unchanged; without a sandbox it does nothing and gives `Fresh(NoSaveEver)`. The runner uses the start to tell the agent what was kept; the harness writes no such text. If the future of the lifecycle `open` is dropped, no box of the instance stays open and the instance can be opened again; a `close` that comes while an `open` is in progress leaves no box open when the open ends.

`close(instance, RunEnd)` ends the run, whatever the cause (`Completed`, `Failed`, `Cancelled`). With a sandbox, after `Completed` it stops the background commands, then saves the work folder, then releases the box; after `Failed` or `Cancelled` it stops the background commands, then releases the box, and does not save. Open and close are per instance. This crate holds the trait only.

## Rules between ports

The harness enforces them where ports meet; the provider does not get them. Deny by default: from the runner side into the box only the attachments of a mail under the gate of `get_mail_files`, the skill files, the seed that the runner gives and the web access setting cross.

1. The harness never puts the runner credential in the box: in no file it writes, no command it runs and no open request it makes.
2. It copies the attachments of a mail into the work folder only for a mail that passes the gate.
3. The web access setting goes to the sandbox unchanged; no setting, no network.
4. The turn reference and the seed of the run go to the open request unchanged.
5. The work folder is saved only after a completed run.

The gate of `get_mail_files` accepts only the mail ids that the harness history adapter has seen (on the first read of the history and on every write) and fails closed. It is harness logic and adds no item here. Three conditions the harness cannot check: a runner whose box has internet holds no internal data in the box; the seed that the runner gives holds no runner credential; a provider-side store of the work folder must not go around the per-thread authorization of Threads.

## In-memory ports (`memory`)

For tests, not for production. One `HarnessRecord` keeps the calls of every in-memory port, in order (`HarnessCall`, non-exhaustive).

- `MemoryHarnessHistory::new(&record, thread, access)` wraps a `MemoryThreadAccess` of `br-agentic-contract`. An entry in the conversation is `Stored`; `expect` registers an entry that is not stored yet (the wait stays pending), `drop_entry` makes it `Dropped` (an entry the conversation already holds stays `Stored`), `close_channel` makes pending waits a fault; any other entry is a fault.
- `MemoryMail::new(&record, incoming)` wraps a `MemoryIncoming`; `put_document` sets the content behind a `DocumentRef`; a document that is not set is a fault.
- `MemorySkills::new(&record).with_skill(name, description, files)`.
- `MemorySandbox::new(&record)`: foreground commands answer the result set with `script` (exit code 127 otherwise). It keeps the saves of all its boxes by thread, agent and turn, as a provider does, and follows the rules of the port above; it gives `Continued`, `Restored` and each of the five `Fresh` reasons. A saved and released box is kept for the next open of the same thread and agent that has the saved turn as parent (`Continued`, the same `BoxId`, the work folder of the save) until `end_keep(&turn)`; then that open is `Restored`. One open per instance at a time: a second `open` of an instance whose box is not released is a refusal. On a released box, `run`, `read`, `write`, `list`, `kill` and `save` are faults, and `release` is accepted again with no effect; on a box never opened every operation, `release` included, is a fault.
- Looking: `network` (`Network`: `Closed`, or `Open(WebAccess)`; no setting, no access and an empty list of sites are closed), `running`, `files` (the work folder now), `seed` (the seed of the box), `saved` (the saved copy under the turn of the box), `save_of(&turn)`, `writes` and `is_released`.
- Arranging a start: `expire_save(&turn, expired_at)` (makes `SaveExpired`, also while the box is alive), `fail_save(&turn)` (the save of that turn did not finish: `save` is a fault, the next start is `SaveFailed`), `fail_restore(&turn)` (a restore of that save fails: `RestoreFailed`; a kept box is still `Continued`), `make_store_unavailable()` (`open` of a turn with a parent gives `StoreUnavailable`, `save` is a fault), `forget_save(&turn)` (a test arrangement: the provider holds no record of that save any more, so the next start is `Fresh(NoSaveEver)` and a save of that turn is accepted again), `end_keep(&turn)`, `set_clock(time)` (a test arrangement: the time that the next `SaveReceipt` values carry; the clock of the host by default) and `end_process`.
- Arranging an error of open: `fail_next_open(OpenFault)` makes the next `open` fail with that fault and claims nothing. `hold_next_open()` gives an `OpenHold`: the next `open` makes its box, then waits; `box_made().await` gives the box, `proceed()` lets the open end, and dropping the open future instead releases the box, so that the next open of the same instance succeeds.
- The record: `HarnessCall::SandboxOpen` keeps each open request in full (turn reference, seed, and the bytes of the seed files), `SandboxWrite` each write with its path and its bytes (a file written and then replaced stays in the record with the bytes that were written) and `SandboxRun` each command with its text. The rule "no runner credential in the box" is checkable: a test searches a canary in the recorded writes, commands and open requests and in the files and the seed of the box.

The tests `mark_after_stored`, `lifecycle_order`, `lifecycle_open`, `lifecycle_interrupted` (a dropped open, a close during an open) and `rules` run drivers written inside the tests. They are contract examples: they show that the doubles carry the sequence, not that a harness does it. The proof of the harness rules (mark after `Stored`, close order, the gate, the credential rule, the open request, save only after a completed run, deny by default) lives in `br-agentic-harness`.

## Why

| Thing | Why it is the way it is |
|---|---|
| `HarnessHistory` is a subtrait | a supertrait would make `br-agentic-contract` depend on this crate: a cycle |
| `PromptPort` and `ToolsPort` are re-exported | a second declaration would be a second type |
| unknown is never `Stored` | an adapter built per run would else mark mails read that were never stored, and lose them |
| `OpenRequest.web_access` is an `Option` | the node may have no setting; no setting must mean no network, not full access |
| `OpenRequest`, `TurnRef` and the results are `#[non_exhaustive]` | a later minor adds a field (the image name) without a break |
| the save refusal has a fixed text | `PortError::Refused` holds a text only; a typed test needs a value that cannot be mistaken, and the text is part of the contract |
| the two open errors are causes of `PortError::Fault` | the other ports and the harness keep one error type; a refusal means an expected case, these are not |
| the content hash has a domain line and length prefixes | no two different seeds can give the same byte stream, and the value must not move inside the minor |
| the in-memory sandbox refuses a second open of a live instance | else "a dropped open must not block the next open" cannot fail in a test |
| `SandboxPort` is shaped on process and file calls | so that the provider can change |
| the lifecycle, the gate and the rules are not code here | they are logic of the harness; this crate holds contracts and doubles |

## Install

Not on crates.io. Pin the git tag with the matching `version` beside it, and pin `br-agentic-contract` and `br-llm-messages` on the same git URLs and tags as this crate, or the types split in two copies:

```toml
br-agentic-harness-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-harness-contract", tag = "br-agentic-harness-contract/v0.2.0", version = "0.2.0" }
br-agentic-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-contract", tag = "br-agentic-contract/v0.3.0", version = "0.3.0" }
br-llm-messages = { git = "https://github.com/BotResources/br-llm-messages-rs", package = "br-llm-messages", tag = "br-llm-messages/v2.1.0", version = "2.1.0" }
```
