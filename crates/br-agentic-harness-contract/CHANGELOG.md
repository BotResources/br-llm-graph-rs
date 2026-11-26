# Changelog

All notable changes to `br-agentic-harness-contract` are documented here. The git tag
`br-agentic-harness-contract/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.2.0 - 2026-11-26

Breaking minor of the sandbox port and of the run lifecycle, to the sealed intent of minor 0.2: the work folder of an agent can be kept from one turn to the next. The history, mail, skills, prompt and tools ports, the stored signal and the gate of `get_mail_files` do not change. Dependents to move: `br-agentic-harness`, `br-runner-jobs`, `br-runner-threads`. Every implementer of the sandbox port, such as a provider adapter, moves with this release to the new `open` and `save` signatures of the sandbox port, and to the new `RunLifecycle::open` signature where it implements the lifecycle.

### Added

- `OpenRequest` (`instance`, `web_access`, `turn`, `seed`; `#[non_exhaustive]`, made by `OpenRequest::new`, no image field), `Opened` (`box_id`, `start`), `TurnRef` (thread, agent, turn id, parent turn id), `SaveReceipt` (turn, time, raw size in bytes).
- `Seed`, `SeedEntry`, `SeedKind` (file, directory, symlink), `InvalidSeed` and `ContentHash`: no two entries with one path, the empty seed is valid, the content hash depends on the entries only and is frozen for the minor.
- `StartKind` (`Continued`, `Restored`, `Fresh`) and `FreshReason` (`NoSaveEver`, `SaveExpired`, `SaveFailed`, `RestoreFailed`, `StoreUnavailable`).
- `OpenFault` (`ImageUnavailable`, `ImageFailing`), carried as the cause of `PortError::Fault`, with `OpenFault::of(&PortError)` as the typed test; `AlreadySaved` with `AlreadySaved::is(&PortError)` and `AlreadySaved::refusal()`.
- Re-exports of `AgentId` and `TurnId` of `br-llm-messages`.
- In-memory sandbox: the saves of all boxes by thread, agent and turn; the `Continued` keep of a saved and released box; arrangements `expire_save`, `fail_save`, `fail_restore`, `forget_save`, `make_store_unavailable`, `end_keep`, `fail_next_open`, `hold_next_open` (`OpenHold`) and `set_clock`; lookups `seed` and `save_of`.

### Changed

- `SandboxPort::open` takes `&OpenRequest` and gives `Opened`; `SandboxPort::save` gives a `SaveReceipt`. Breaking.
- `RunLifecycle::open` takes the instance, the `TurnRef` and the `Seed` and gives the `StartKind`; without a sandbox the start is `Fresh(NoSaveEver)`. After `Failed` or `Cancelled` the harness releases the box and does not save. Breaking.
- `HarnessCall::SandboxOpen` holds the whole `OpenRequest`; `HarnessCall::SandboxWrite` holds the bytes written. A test can search a canary credential in the recorded open requests, writes and commands.
- `MemorySandbox` refuses a second `open` of an instance whose box is not released, and releases what a dropped `open` future claimed.
- New dependency: `sha2` (content hash).

### Fixed

- The rule "no runner credential in the box" has a test that searches a canary credential in the recorded writes, commands and open requests and in the files and the seed of the box.

### Documentation

- README: the open request, the seed, the start, the save, the errors of open, the lifecycle, rules 4 and 5, and the in-memory arrangements.

## 0.1.1 - 2026-11-23

Patch of minor 0.1, additive only.

### Added

- `MemorySandbox::files` and `MemorySandbox::writes`: the current content of a box and every write to it in order, so a test can assert that a value never reached a box.

### Fixed

- `MemoryHarnessHistory` answers `Stored` for an entry the conversation holds, also after `drop_entry` on it.

### Documentation

- README: the tests `mark_after_stored`, `lifecycle_order` and `rules` are contract examples, the proof of the harness rules is in `br-agentic-harness`; the `Network` type is described.

## 0.1.0 - 2026-11-23

First release, to the sealed intent of minor 0.1, on `br-agentic-contract` 0.3.0.

### Added

- `HarnessHistory`, a subtrait of `ThreadAccess` with `stored(thread, entry)`, and `StoredAnswer` (`Stored`, `Dropped`); `PromptPort` and `ToolsPort` re-exported.
- `MailPort` (`items`, `mark_read`, `send`, `documents`), `SkillsPort` with `SkillName` and `SkillSummary`, `SandboxPort` with `BoxId`, `ProcessId`, `Command`, `RunMode`, `CommandResult`, `RunOutcome`, and `FilePath`.
- `RunLifecycle` (`open`, `close`) and `RunEnd`.
- In-memory ports in `memory`: `MemoryHarnessHistory`, `MemoryMail`, `MemorySkills`, `MemorySandbox`, with `HarnessRecord` and `HarnessCall`.
