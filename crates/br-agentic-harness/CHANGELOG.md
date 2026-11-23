# Changelog

All notable changes to `br-agentic-harness` are documented here. The git tag
`br-agentic-harness/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

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
