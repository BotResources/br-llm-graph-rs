# br-agentic-harness-contract

What the harness of an agentic node asks from a runner, in hexagonal terms. The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

- The harness is an optional layer between an agentic node and the runner that runs it. The node keeps seeing the node ports of `br-agentic-contract`; the harness fills them on one side and asks the ports of this crate from the runner on the other side.
- This crate is a contract library: port traits, the plain types of their signatures, the errors that cross them, and in-memory ports for tests. It holds no code that does a job through a port; the harness logic and the mail tools are in `br-agentic-harness`.
- Importers: the harness, and the adapter libraries of a runner (history, mail, sandbox, skills). A runner that needs no harness does not import it.

## The six ports

All ports are public, object-safe, `Send + Sync` and asynchronous (`PortFuture` of `br-agentic-contract`). They share its error, `PortError`: `Refused(Reason)` for an expected error, `Fault(Cause)` for anything else. A port can serve several instances at once and never decides policy.

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

`stored(thread, entry)` answers `StoredAnswer::Stored` (the thread holds the entry) or `StoredAnswer::Dropped` (it never will). An entry the adapter has no record of, a thread it has no slot for, and a closed change channel are a `PortError`, never `Stored`. The future ends when the answer is known; dropping it loses nothing.

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
- `SandboxPort::open(instance, web)` gives a `BoxId`; `web` is the `WebAccess` of the node, passed unchanged, and `None` means no setting, hence no network. The sandbox enforces it, the harness never does.
- `run(box, command, mode)` gives `RunOutcome::Finished(CommandResult)` for `RunMode::Foreground` and `RunOutcome::Started(ProcessId)` at once for `RunMode::Background`. `read` of a missing file is a refusal. `kill` of an ended process changes nothing. `save` keeps the work folder; `release` ends the use of the box. How a provider builds, keeps or deletes a box is not the concern of this crate.
- `FilePath` is a relative path that stays in the folder (no absolute path, no `..`, no backslash, no NUL). `SkillName` and `Command` are non-blank.
- The sandbox and skills ports are optional for a runner: with none, the harness does mail and history only.

## Run lifecycle

`RunLifecycle` is not a port of the runner: the runner calls it towards the harness. `open(instance)` starts the run; `close(instance, RunEnd)` ends it, whatever the cause (`Completed`, `Failed`, `Cancelled`). With a sandbox, `close` stops the background commands, then saves the work folder, then releases the box. Open and close are per instance. This crate holds the trait only.

## Rules between ports

The harness enforces them where ports meet; the provider does not get them. Deny by default: from the runner side into the box only the attachments of a mail under the gate of `get_mail_files`, the skill files and the web access setting cross.

1. The harness never puts the runner credential in the box.
2. It copies the attachments of a mail into the work folder only for a mail that passes the gate.
3. The web access setting goes to the sandbox unchanged; no setting, no network.

The gate of `get_mail_files` accepts only the mail ids that the harness history adapter has seen (on the first read of the history and on every write) and fails closed. It is harness logic and adds no item here. Two conditions the harness cannot check: a runner whose box has internet holds no internal data in the box; a provider-side store of the work folder must not go around the per-thread authorization of Threads.

## In-memory ports (`memory`)

For tests, not for production. One `HarnessRecord` keeps the calls of every in-memory port, in order (`HarnessCall`, non-exhaustive).

- `MemoryHarnessHistory::new(&record, thread, access)` wraps a `MemoryThreadAccess` of `br-agentic-contract`. An entry in the conversation is `Stored`; `expect` registers an entry that is not stored yet (the wait stays pending), `drop_entry` makes it `Dropped`, `close_channel` makes pending waits a fault; any other entry is a fault.
- `MemoryMail::new(&record, incoming)` wraps a `MemoryIncoming`; `put_document` sets the content behind a `DocumentRef`; a document that is not set is a fault.
- `MemorySkills::new(&record).with_skill(name, description, files)`.
- `MemorySandbox::new(&record)`: foreground commands answer the result set with `script` (exit code 127 otherwise); `network`, `running`, `end_process`, `saved` and `is_released` let a test look. A released box faults on every operation; a box never opened faults too.

## Why

| Thing | Why it is the way it is |
|---|---|
| `HarnessHistory` is a subtrait | a supertrait would make `br-agentic-contract` depend on this crate: a cycle |
| `PromptPort` and `ToolsPort` are re-exported | a second declaration would be a second type |
| unknown is never `Stored` | an adapter built per run would else mark mails read that were never stored, and lose them |
| `open` takes `Option<&WebAccess>` | the node may have no setting; no setting must mean no network, not full access |
| `SandboxPort` is shaped on process and file calls | so that the provider can change |
| the lifecycle, the gate and the rules are not code here | they are logic of the harness; this crate holds contracts and doubles |

## Install

Not on crates.io. Pin the git tag with the matching `version` beside it, and pin `br-agentic-contract` and `br-llm-messages` on the same git URLs and tags as this crate, or the types split in two copies:

```toml
br-agentic-harness-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-harness-contract", tag = "br-agentic-harness-contract/v0.1.0", version = "0.1.0" }
br-agentic-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-contract", tag = "br-agentic-contract/v0.3.0", version = "0.3.0" }
br-llm-messages = { git = "https://github.com/BotResources/br-llm-messages-rs", package = "br-llm-messages", tag = "br-llm-messages/v2.1.0", version = "2.1.0" }
```
