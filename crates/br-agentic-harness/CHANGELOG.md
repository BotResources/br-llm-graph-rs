# Changelog

All notable changes to `br-agentic-harness` are documented here. The git tag
`br-agentic-harness/v{version}` releases the crate. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings have the form `## X.Y.Z - date` — the release pipeline greps
the `## X.Y.Z` prefix to decide whether a version ships.

## Unreleased

## 0.1.0 - 2026-11-23

First release, to the sealed intent of minor 0.1, on `br-agentic-contract` 0.3.0 and `br-agentic-harness-contract` 0.1.0.

### Added

- `Harness` and `HarnessBuilder`: built from the history, prompt and tools ports of a runner, with the mail, skills and sandbox ports optional; it gives the node the history, prompt, tools and incoming ports.
- `HistoryAdapter`, `PromptAdapter`, `ToolsAdapter` and `IncomingAdapter`; the history adapter fills the mail set from the first read and every write.
- The mail tools `send_mail` and `get_mail_files` (`SEND_MAIL`, `GET_MAIL_FILES`), which replace those of `br-agentic-nodes` 0.3; `get_mail_files` fails closed on an id outside the mail set.
- The mail gate: a mail is marked read only after the stored signal says `Stored`, in the background; `Dropped`, an unknown entry or thread, a closed channel, a fault and a refusal leave it unread.
- `RunLifecycle` for `Harness`: `open` passes the web access setting unchanged; `close` stops the tracked background commands, saves the work folder and releases the box, whatever the cause. `Harness::track_background` registers a background command of an open instance.
