# Changelog

All notable changes to `br-agentic-harness` are documented here. The git tag
`br-agentic-harness/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.2.0 - 2026-11-26

Breaking minor, to the sealed intent of minor 0.2, on `br-agentic-contract` 0.3.0 and `br-agentic-harness-contract` 0.2.0: the work folder of an agent is kept from one turn to the next only when the turn completed. The crate moves to the tag `br-agentic-harness-contract/v0.2.0`; a runner that uses the harness resolves one copy of the contract.

### Changed

- `RunLifecycle::open` takes the instance, the `TurnRef` and the `Seed` and gives the `StartKind` that the sandbox port answers, unchanged. The harness builds one `OpenRequest` from the instance, the web access setting, the turn reference and the seed, and gives it to the sandbox port. With no sandbox port, `open` ignores the turn and the seed and gives `Fresh(NoSaveEver)`. Breaking.
- `close` saves the work folder only when the end is `Completed`. After `Failed` or `Cancelled` it stops the background commands and releases the box, and never saves. Breaking in behaviour.
- A save that the port refuses with `AlreadySaved` is taken as done in `close`; any other failure of the save is a failure of `close`.
- A `close` that arrives while an `open` is in flight keeps its end; when the port answers that `open` with a box, the box is ended by the rule of that end (a save only after `Completed`), then released. When two closes arrive, the first end decides. The `open` then answers `PortError::Refused`, also when a step of ending the box fails; the failure of that step is not returned by the `open`.

### Known limit

- 0.2.0 closes: an `open` whose future is dropped before the port answers (no reservation is kept, no box is made, a later `open` is accepted), and a `close` in flight during an `open` (the box is ended by the end of that `close`). A cancel that lands while a box is being ended, after a `close` met an `open` (the future of the `open` or of the `close` is dropped during the kill, save or release), can stop the remaining steps and leave the box. This is a known limit left for a later minor.

### Tests

- A fault of the sandbox port at `open` is returned unchanged, so `OpenFault::ImageUnavailable` and `OpenFault::ImageFailing` reach the runner as two distinct kinds. The code line is the same as in 0.1.1; the tests are new.
- The sandbox doubles move to the 0.2.0 port. New tests prove: the single open request and what it holds, the start kinds, the open faults, the save only after `Completed`, `AlreadySaved`, a dropped open, a close in flight (each end), two closes, a fault answered to an open that a close met, and the refusal that the open answers when ending its box fails.
- The credential test puts the canary in the prompt, a tool name and description, a mail, a skill and the history, and searches every call that reaches the sandbox port, its files, its saves and its seed.

## 0.1.1 - 2026-11-23

Patch of the audit of 0.1.0 (findings F1, F2, F3, F6); no public surface change.

### Fixed

- A dropped or cancelled `open` gave its reservation back never, so every later `open` of the instance was refused; a reservation guard now releases it.
- A `close` during an `open` in flight found no run and the box that `open` then committed was never released; that `open` now saves and releases the box and answers `PortError::Refused`.
- `close` clears the mail set of the threads of the instance (see the README).

### Tests

- A doc-test proves that `RunContext` cannot be built by a struct literal outside its crate (`E0639`).
- The credential test is rewritten: the secret is in the prompt text, a tool name, a mail subject and body, an attachment name and an attachment body; every sandbox call is asserted on its full content.

## 0.1.0 - 2026-11-23

First release, to the sealed intent of minor 0.1, on `br-agentic-contract` 0.3.0 and `br-agentic-harness-contract` 0.1.0.

### Added

- `Harness` and `HarnessBuilder`: built from the history, prompt and tools ports of a runner, with the mail, skills and sandbox ports optional; it gives the node the history, prompt, tools and incoming ports.
- `HistoryAdapter`, `PromptAdapter`, `ToolsAdapter` and `IncomingAdapter`; the history adapter fills the mail set from the first read and every write.
- The mail tools `send_mail` and `get_mail_files` (`SEND_MAIL`, `GET_MAIL_FILES`), which replace those of `br-agentic-nodes` 0.3; `get_mail_files` fails closed on an id outside the mail set.
- The mail gate: a mail is marked read only after the stored signal says `Stored`, in the background; `Dropped`, an unknown entry or thread, a closed channel, a fault and a refusal leave it unread.
- `RunLifecycle` for `Harness`: `open` passes the web access setting unchanged; `close` stops the tracked background commands, saves the work folder and releases the box, whatever the cause. `Harness::track_background` registers a background command of an open instance.
