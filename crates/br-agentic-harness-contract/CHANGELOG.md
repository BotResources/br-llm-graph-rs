# Changelog

All notable changes to `br-agentic-harness-contract` are documented here. The git tag
`br-agentic-harness-contract/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.1.0 - 2026-11-23

First release, to the sealed intent of minor 0.1, on `br-agentic-contract` 0.3.0.

### Added

- `HarnessHistory`, a subtrait of `ThreadAccess` with `stored(thread, entry)`, and `StoredAnswer` (`Stored`, `Dropped`); `PromptPort` and `ToolsPort` re-exported.
- `MailPort` (`items`, `mark_read`, `send`, `documents`), `SkillsPort` with `SkillName` and `SkillSummary`, `SandboxPort` with `BoxId`, `ProcessId`, `Command`, `RunMode`, `CommandResult`, `RunOutcome`, and `FilePath`.
- `RunLifecycle` (`open`, `close`) and `RunEnd`.
- In-memory ports in `memory`: `MemoryHarnessHistory`, `MemoryMail`, `MemorySkills`, `MemorySandbox`, with `HarnessRecord` and `HarnessCall`.
