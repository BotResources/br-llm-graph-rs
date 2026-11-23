# br-agentic-harness

The logic of the harness of an agentic node. The sealed intent of the minor is in the BotResources libraries registry. This file says what the code gives.

- The harness sits between an agentic node and the runner that runs it. It is built from the harness ports of `br-agentic-harness-contract` that the runner gives, and it gives the node four node ports of `br-agentic-contract`: history, prompt, tools and incoming.
- It defines no port and no seam. It reads and writes nothing itself: every effect goes through a harness port. It knows no runner library, no service and no provider.
- It needs a Tokio runtime (the wait for the stored signal runs as a background task).
- A runner that needs no mail logic does not import it.

## Build

```text
let harness = Harness::builder(history, prompt, tools)
    .mail(mail)
    .sandbox(sandbox)
    .skills(skills)
    .web_access(web)
    .build();
```

`history` is a `HarnessHistory`, `prompt` a `PromptPort<F>`, `tools` a `ToolsPort<F>`; they are required. `mail`, `sandbox`, `skills` and `web_access` are optional. For an absent port the harness adds no tool and no prompt text, and does not fail. No mail port: no mail tool and `incoming()` is `None`. No sandbox port: `open` and `close` do nothing to a sandbox. The skills port is kept (`skills()`) and not used in 0.1.

| Accessor | Gives the node |
|---|---|
| `history()` | `Arc<dyn ThreadAccess>`: the behaviour of `HarnessHistory`; every entry read or written adds its mail ids to the mail set |
| `prompt()` | `Arc<dyn PromptPort<F>>`: the prompt of the runner, unchanged |
| `tools()` | `Arc<dyn ToolsPort<F>>`: the tools of the runner plus the two mail tools |
| `incoming()` | `Option<Arc<dyn IncomingPort>>`: `items` from the mail port; `mark_read` behind the stored signal |

`Harness` implements `RunLifecycle`; the runner calls `open(instance)` and `close(instance, end)` towards it.

## Mail set and mail gate

- The mail set holds the ids of the mail entries (`AgentMail`) of each thread. It is filled only from the history of the run: on every `read`, `add_input_turn` and `add_entry` through the history adapter, once the write succeeded. Mail listed by the mail port is never added.
- `close` clears the mail set of every thread that the tools port saw for that instance (the thread of the run context of each tools call); a thread that no tools call named stays in the set. A mail of the thread is accepted again only after the history adapter reads or writes it again.
- `get_mail_files` accepts only an id of the mail set of the thread of the run context. Any other id gets the same error text, asks the mail port nothing and reveals nothing. The gate is not in `RunContext`; the harness keeps it.
- The incoming adapter marks a mail read through the mail port only after `stored(thread, entry)` answers `Stored`. `mark_read` returns at once; a background task waits and marks. `Dropped`, a fault, a refusal, an unknown entry or thread and a closed channel leave the mail unread, so that it comes again. The waiting tasks end when the last handle of the incoming port (the harness and every `Arc` clone given by `Harness::incoming()`) is dropped. A crash between the write and the mark can deliver a mail twice and cannot lose it; this holds for mail only.

## Tools

| Tool | What it does |
|---|---|
| `send_mail` | `to` (at least one agent), `subject`, `body`; optional `cc` and `attachments` (`name`, `document`). The author, the sender agent and the source conversation come from the run context. Answers with the id of the mail |
| `get_mail_files` | `mail_id`; gives each attachment as a text block, a binary one as its name and size. With an open box, also copies each attachment to `mail/{mail id}/{name}` of the work folder; a name that is not a safe relative path is not copied and the result says so |

Names, texts and answers are those of the mail tools of `br-agentic-nodes` 0.3. The mail tools are built per call of the tools port from the `RunContext` of that call. A runner tool that has the name of a mail tool makes the call refuse (`PortError::Refused`, naming the clash); nothing is dropped silently. A fault of the runner tools port is returned as is. A `RunContext` has only required fields, so it cannot lack a value a mail tool needs: there is no fault path for that case. An attachment whose name is empty, `.`, `..` or holds a path separator is not copied to the work folder (it is listed as left out; the text result is still given). Two concurrent opens of one instance open one box; the other is refused. An argument error or a port refusal is an error result of the tool, never a panic.

## Run lifecycle and rules between ports

- `open`, with a sandbox port: `SandboxPort::open(instance, web)`; `web` is the `WebAccess` given to the builder, unchanged, and `None` when the node gave none. The harness never enforces it and never adds one. An instance already open is refused.
- `close`, with a sandbox port, whatever the `RunEnd`: `kill` of each process registered with `track_background`, then `save`, then `release`. A failed step does not skip the next ones; the first failure is returned. A close with no open box does nothing. A close that arrives while an `open` is in flight makes that `open` save and release the box it opened, and it answers `PortError::Refused`. A dropped or cancelled `open` gives its reservation back, so the instance can be opened again.
- Deny by default: into the box go only the web access setting, the attachments of a mail that passes the gate of `get_mail_files`, and nothing else. The harness never writes the runner credential, a prompt, a tool or a skill file to the box.
- The harness cannot check two rules of the contract: a runner with internet holds no internal data in the box (runner author), and a provider-side store of the work folder does not go around the per-thread authorization of Threads (provider).

## Not in 0.1

The box tools (run, read, write, list), the description of the box in the prompt, the skill files in the box and the list of skills in the prompt. The harness builds no sandbox tool and no skills tool.

## Why

| Thing | Why it is the way it is |
|---|---|
| the mail set is per thread | a harness that serves several threads must not let a mail seen on one thread open the files on another |
| the mail set holds `AgentMail` entries only | a copy notice and a runtime notice carry no attachment the agent may fetch; an id outside the set fails closed |
| `mark_read` spawns a task | the node must not wait for the history; a task that outlives the harness would mark after the run ended, so the incoming adapter aborts its tasks when its last handle is dropped |
| an unknown answer is never `Stored` | a mail marked read before its entry is stored is lost on a crash |
| `track_background` exists | the port gives no list of processes and the harness builds no run tool in 0.1, so the runner that starts a background command tells the harness |
| `close` goes on after a failed step | a box that is not released is a leak; a folder that is not saved is lost work |
| copy to `mail/{id}/{name}` | the place that the reference driver of `br-agentic-harness-contract` uses; the copy is best effort and the text result is always given |
| `RunContext` is only received | it is `#[non_exhaustive]`; the harness never builds one, the runner does with `RunContext::new`; the doc-test below proves that a struct literal does not compile outside its crate |

```compile_fail,E0639
# use br_agentic_contract::{InstanceId, RunContext, ThreadRef};
# use br_llm_messages::{AgentId, Author, ConversationId};
# fn literal(
#     instance: InstanceId,
#     thread: ThreadRef,
#     agent: AgentId,
#     author: Author,
#     conversation: ConversationId,
# ) -> RunContext {
RunContext { instance, thread, agent, author, conversation }
# }
```

## Install

Not on crates.io. Pin the git tag with the matching `version` beside it, and pin the other libraries on the same git URLs and tags as this crate, or the types split in two copies:

```toml
br-agentic-harness = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-harness", tag = "br-agentic-harness/v0.1.1", version = "0.1.1" }
br-agentic-harness-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-harness-contract", tag = "br-agentic-harness-contract/v0.1.0", version = "0.1.0" }
br-agentic-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-agentic-contract", tag = "br-agentic-contract/v0.3.0", version = "0.3.0" }
br-graph-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-graph-contract", tag = "br-graph-contract/v0.1.0", version = "0.1.0" }
br-llm-clients-core = { git = "https://github.com/BotResources/br-llm-clients-rs", package = "br-llm-clients-core", tag = "br-llm-clients-core/v0.5.0", version = "0.5.0" }
br-llm-messages = { git = "https://github.com/BotResources/br-llm-messages-rs", package = "br-llm-messages", tag = "br-llm-messages/v2.1.0", version = "2.1.0" }
```
