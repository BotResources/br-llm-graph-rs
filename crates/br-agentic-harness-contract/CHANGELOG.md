# Changelog

All notable changes to `br-agentic-harness-contract` are documented here. The git tag
`br-agentic-harness-contract/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

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
