# br-llm-graph

LangGraph-like orchestration over br-llm-messages and br-llm-clients-core: a typed key/value state read and written through typed keys, async nodes, sync edges that may declare their targets, supersteps with fan-out and join, calls of graphs and maps over lists, resumption from checkpoints, a node that makes one model call, a ReAct agent and a generator-critic, and a description of every built graph as data. The model is a `Client` of br-llm-clients-core the host gives; the tools, the hooks that judge their calls and the thread store an agent works in are the host's. Not on crates.io; pin the git tag `{crate}/v{version}` with the matching `version` beside it. The graph's API is made of br-llm-messages and br-llm-clients-core types (authors, conversations, `Client`, `ToolSpec`, `OutputMode`), so a dependent pins them on the tags the graph builds on, beside it:

```toml
[dependencies]
br-llm-graph = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-llm-graph", tag = "br-llm-graph/v0.6.0", version = "0.6.0" }
br-llm-clients-core = { git = "https://github.com/BotResources/br-llm-clients-rs", tag = "br-llm-clients-core/v0.4.0", version = "0.4.0" }
br-llm-messages = { git = "https://github.com/BotResources/br-llm-messages-rs", tag = "br-llm-messages/v0.5.0", version = "0.5.0" }

[dev-dependencies]
br-llm-clients-mock = { git = "https://github.com/BotResources/br-llm-clients-rs", tag = "br-llm-clients-core/v0.4.0", version = "0.4.0" }
```

Every other crate of br-llm-clients-rs used beside the graph (the mock, a provider library) is pinned on the same tag as br-llm-clients-core, `br-llm-clients-core/v0.4.0`, never on a tag of its own: a git dependency's source is its tag, so a crate pinned on another tag of that repository builds a second br-llm-clients-core, whose `Client` is not the graph's (E0277, "multiple different versions of crate `br_llm_clients_core`"), and `[patch]` does not unify two tags of one repository.

## Usage

A schema gives typed keys; reads, updates and mappings through them take only values and keys of their kind, so a wrong kind does not compile. Keys by name (`Key`, `Value`, `Update`) stay for states and checkpoints reloaded from JSON and for graphs described as data.

```rust
let mut schema = Schema::builder();
let topic: StateKey<String> = schema.state("topic")?;
let words: StateKey<i64> = schema.state("words")?;
let schema = schema.build();

let count = FnNode::new(move |state: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
    let update = state.get(&topic).and_then(|t| words.set(t.split_whitespace().count() as i64));
    Box::pin(async move { Ok(vec![update?]) })
});
```

A runner gives a model-calling node its model, its system and its history, each a value or a function of the node's context that declares the keys it reads. An agent built with a thread store is durable: it works in the thread its input `thread` names (the empty string asks the store to create one), resumes from it after an interruption and gives the thread's id back; without one it works in memory, for the current call only. Every new thread id and turn id comes from the host's `IdSource`, asked under the run id, the agent's occurrence, the user input its call answers (its trigger) and the rank of the id: a deterministic `IdSource` (one that gives the same id for the same request, such as a hash of it) makes the same run id plus the same trigger the same call across invocations, so a call run again after a crash finds the thread it wrote and goes on from it.

```rust
let agent = ReactAgent {
    author: Author::new("agent", "agent")?,
    model: Model::client(client),                    // an Arc<dyn br_llm_clients_core::Client>
    system: Prompt::from_config(&base),              // a ConfigKey<String> of the agent's graph
    history: None,                                   // its conversation rendered for its author
    output_mode: OutputMode::Plain,
    tools: vec![Arc::new(SearchTool::new())],
    tool_nodes: Vec::new(),
    tool_concurrency: None,
    round_limit: None,
    enabled: None,
    inputs: Vec::new(),
    thread_store: Some(store),                       // the host's ThreadStore
}
.graph()?;
// Inputs `opening` (user inputs ending with the one the call answers) and
// `thread`; outputs `answer` and `thread`; called with `SubGraph` or run.
```

A built graph describes itself as data, without running anything: its entry, its signature, its schema and its nodes in registration order, each with its kind, its realizes labels, the targets its edge declares and the fields of its kind, nested graphs included (the child of a call, the body of a map, an agent's own graph). A host or a CI job compares it with the graph it meant to build, such as a Runner's intent graph; the library compares and draws nothing. An edge built from a function declares its targets with `FnEdge::targets` (an `Always` declares its own): the build refuses a declared node the graph does not hold (`EdgeToUnknownNode`) and the run refuses a target the edge did not declare (`UndeclaredTarget`), so the description cannot be false. A description writes no state value, default, prompt, client, author name or secret: a subject a runner gives is written as how it is given (`fixed`, `ready_made`, `function`), a fixed value mapped into a call and a state key's default as their kinds, an author by its id and never its name, and no tool argument is written. It does write the graph's fixed bounds: fixed limits and counts, and the generator-critic's `final_generation`.

```rust
let route = FnEdge::new(move |state: &State, _: &Config| {
    Ok(vec![if state.get(&notes)?.is_empty() { skipped.clone() } else { ask.clone() }])
})
.targets([ask.clone(), skipped.clone()]);           // the targets it may give

let graph = GraphBuilder::new(schema)
    .entry(plan_id.clone())
    .node(plan_id.clone(), plan.realizing(["PlanTopics"]))   // the intent element it implements
    .node(summaries_id.clone(), summaries)
    .edge(plan_id, Always(Target::Node(summaries_id.clone())))
    .edge(summaries_id, route)
    // ...
    .build()?;

let json = serde_json::to_string_pretty(&graph.describe())?;  // br-llm-graph-description/1
```

The runnable examples are in `crates/br-llm-graph/examples` (`cargo run -p br-llm-graph --example react_agent`): durable agents over a thread store kept in memory, a generator-critic with a rule check, an `LlmCall` as the body of a map, a hook guarding a tool, a served session, resumption from checkpoints, the description of a graph (`--example describe`).

## br-graph-contract

The contract of the graph family, with no run logic: the state that holds the runner's fields, the node that creates instances, the instance with its separate input and event ends, the edge, the target `END`, the cancel token, and the graph validated as a whole with its description as data (`Structure`). It depends on no executor, no node library and no LLM crate, so an executor, a node library and a runner author each import only it. Not on crates.io; pin the git tag `{crate}/v{version}` with the matching `version` beside it:

```toml
[dependencies]
br-graph-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-graph-contract", tag = "br-graph-contract/v0.1.0", version = "0.1.0" }
```

| Part | What the crate enforces |
|---|---|
| `State` | create, apply (all or nothing) and rebuild check the fields' own validity and a size limit on their JSON form; a refusal for size names the serialized size and the limit |
| `Instance::channel` | `give` is ordered against the close: an input is accepted before it, or refused as `InputClosed`; at most one terminal `Event` (`finish` consumes the sender); a dropped end ends the instance; `EventSender::dropped` gives the owned future `Dropped` (`Send`, `'static`) |
| `EventEnd::recv` | `Err(EventEndError::Finished)`: the terminal event was already delivered, or the executor dropped the instance; `Err(EventEndError::Vanished)`: the `EventSender` was dropped without `finish`, a node fault the executor must treat as a failure; a dropped `recv` future loses no event |
| `GraphBuilder`, `Structure` | one set of build rules shared by the build and the rebuild of a structure; a `Graph` or a `Structure` that exists is valid; the JSON of a structure is accepted only as objects, a positional array is `StructureError::Malformed` |
| `Graph::evaluate_edge` | targets declared by the edge, no `END` mixed with nodes, no repeats, in declaration order |
| `CancelToken` | never un-triggered; a child is triggered by its parent and never the reverse; a child keeps its parent alive, so a trigger of any ancestor reaches every live descendant; `CancelView::triggered` gives the owned future `Triggered` |

Behaviour to know:

- The library guarantees at most one terminal `Event`, not exactly one: an instance that never calls `finish` stays silent, or ends with `EventEndError::Vanished` if its `EventSender` is dropped.
- Dropping the `InputEnd` or the `EventEnd` drops the instance: the input queue is cleared, `give` is refused as `InputClosed`, `EventSender::is_dropped` is true, and an event sent afterwards is discarded. An event sent before stays receivable on a held `EventEnd` when only the `InputEnd` is dropped; dropping the `EventEnd` discards it.
- Dropping the `EventSender` without `finish` ends the event end without an event and closes the input end. Dropping the `InputReader` closes the input end.
- An accepted input stays readable until the instance finishes or is dropped; the unread accepted inputs are then discarded without a signal. The input queue is unbounded.
- `Node::create` returns an instance that is not checked against the id it was asked for: the executor must check it.
- `Graph::edge_of` returns the raw edge; only `Graph::evaluate_edge` is checked.
- `Target::node`, `Target::from` and the JSON form turn `"END"` into `Target::End`; `Target::Node("END")` can only be built by naming the variant, and a build or a checked evaluation refuses it.
- `CancelToken::trigger` returns after this call has propagated the trigger; a concurrent second call on the same token may return before the first call has finished propagating, but `is_triggered` on a token reports true as soon as the token or any ancestor is triggered, so a descendant never reads untriggered after an ancestor's `trigger` has returned.
- The public API names `uuid::Uuid` (uuid 1.x) and the serde traits (`Fields` requires `Serialize` and `DeserializeOwned`; `Structure` and `Target` implement them).

Not enforced, and left to the implementer as the sealed intent states: that applying the empty update to valid fields is accepted by the fields' own `apply` and leaves them equal (the library cannot tell the empty update from another), that serialize then rebuild gives equal fields (a `NaN` serializes as `null` and cannot be rebuilt, and `State::create` does not catch it), that a copy of the fields equals its original, that applying an update and validating are deterministic, that an update is a delta that stays correct on a later state, that an instance reads its accepted inputs before its terminal event and finishes its work before it, that an instance sends `Cancelled` when its token is triggered, that everything of one instance lives in the instance and not in the node, that dropping an instance stops its work, that a node with no more work sends its terminal event (a silent instance keeps the run alive for ever), that an edge is a pure function of the state, that an instance never changes the state it reads, and that the executor gives each instance a new id and evaluates edges only after a `Result`.

## br-flow-nodes

Two generic programmatic nodes for the control of a graph, built on `br-graph-contract` only: a merger node that waits for lanes that finish at different times, and a map-reduce node that runs one internal node once for each element of a list, with a limit on how many run at the same time, and joins their results in one update. It depends on no executor, no LLM crate and no other node library. Both nodes need a Tokio runtime with tasks: an instance reads its inputs and waits for its cancel token in a task spawned on the runtime of the host. Not on crates.io; pin the git tag `{crate}/v{version}` with the matching `version` beside it:

```toml
[dependencies]
br-flow-nodes = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-flow-nodes", tag = "br-flow-nodes/v0.1.0", version = "0.1.0" }
br-graph-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-graph-contract", tag = "br-graph-contract/v0.1.0", version = "0.1.0" }
```

The public API names types of `br-graph-contract` (`Node`, `State`, `Fields`, `Cause`, `CancelView`), so pin `br-graph-contract` beside it on the same git URL and on the tag `br-graph-contract/v0.1.0`: a git dependency's source is its tag, so a contract pinned on another tag or URL builds a second copy of these types, and the nodes of this crate do not implement the `Node` of the caller.

| Part | What the crate enforces |
|---|---|
| `Merger::new(name, ready, spent)` | kind `programmatic`, subkind `merger`; per input the spent function is called first, then the ready function, on the state of that input alone; when ready, the input end is closed, the accepted inputs are read and one `Result` with the empty update is sent; a triggered cancel token closes the input end and sends `Cancelled` |
| `Merger::late_input(LateInput)`, `LateInput` (`Silent`, `Fail`, `FireAgain`) | the policy a merger is given (`Silent` when none is set): what a merger does with an input whose state the spent function calls spent: stay silent (the default), close the input end and send `Refused` with a reason, or go on to the ready function |
| `MapReduce::new(name, map, internal_node, limit, reduce)` | kind `programmatic`, subkind `map_reduce`; waits for its first input, closes the input end, works on the state of the last accepted input; the limit is a `NonZeroUsize`, so zero cannot be given |
| `MapReduce` work | every initial value becomes an internal state through `State::create_with_limit` (`internal_size_limit`, default `DEFAULT_STATE_LIMIT`); internal instances are created one at a time in list order, at most `limit` live, each with a fresh UUIDv7, a view of the one child cancel token that the outer instance makes from its own, and one input holding its internal state; each internal `Result` is applied with `State::apply`; `reduce` gets the final states in map order and its update is the `Result` |
| `MapReduce` cancel | no other internal instance is started, the live ones are awaited and what they send is discarded, then `Cancelled`; a trigger that is seen once the last internal `Result` is applied and before `reduce` is called gives `Cancelled` (the intent allows `Result` too) |
| `MapReduceError` | the cause of the `Failed` of a map-reduce instance, one named case per fault with the position of the internal state; it names no type of another library, texts are `String` and the cause of the internal node stays the opaque `Cause`; on the first fault the child token is triggered, the other internal instances are dropped and nothing else is sent |

Behaviour to know:

- The functions of both nodes are synchronous and are called on the runtime thread that drives the instance: a blocking call in one blocks that thread. A panic in a function ends the task, so the executor reads `EventEndError::Vanished`; it is not turned into `Failed`.
- A merger keeps nothing between inputs, so an input that finds its state not ready leaves no trace; with `LateInput::Silent` a late input leaves a silent instance and the run stays alive until the host cancels it. A merger whose ready function never gives true does the same.
- A merger with `LateInput::Fail` sends one `Refused` for all the inputs it had accepted. When an input fires the merger, a spent input that is queued behind it is answered by the `Result` and not refused: the inputs read after the close are not judged by the spent function.
- The merger decides on the state of the input it reads. A ready function that is not monotone (true on an older state, false on a newer one) can make the instance fire on a state that a later input has already replaced.
- A map-reduce instance closes its input end when it reads its first input and works on the last input it had accepted: a later input is refused as `InputClosed` and the executor gives it to a new instance.
- `MapReduce::create` and `Merger::create` call `tokio::spawn` when the returned future is first polled, so they must be polled inside a Tokio runtime.
- The internal node is not in the graph, its name is not checked, and a tool that checks a structure cannot see it. An internal instance that stays silent keeps the map-reduce instance, and the run, alive; a trigger of the cancel token stops it.
- Dropping a map-reduce instance (either end) stops its work: the child cancel token is triggered and the internal instances are dropped. The internal instances are held by tasks of a `JoinSet`, so on this path their drop is asynchronous: it happens when the runtime next schedules those tasks, not before the drop of the outer instance returns. On a failure the node triggers the child cancel token, then waits until every other internal instance is dropped, then sends `Failed`, so they are already dropped when the executor reads the event.
- A creation of an internal instance that is in flight when the cancel token is triggered, or when the outer instance is dropped, is abandoned: its pending future is dropped. While a creation is in flight the node keeps reading its live internal instances: a sibling `Result` that arrives is applied, and a sibling fault is acted on at once (the creation is abandoned and the node fails with that fault). Once the node has read a fault it creates no further internal instance.
- An event end of an internal instance that ends because the map-reduce node dropped it is not a fault. If the task that waits for an internal instance ends in any other way (a runtime that shuts down), the outer task ends without an event, which the executor reads as `Vanished`.
- When several internal instances fault at the same time, which fault comes first, and so the cause of `Failed`, depends on task scheduling and is not deterministic.
- An internal node that closes its input end before it receives its one input makes the map-reduce node fail with `MapReduceError::InputRefused`, even if it has already sent a terminal event: the intent states `InputClosed` as a fault, and the node does not read the event before it gives the input.
- `MapReduceError` is `#[non_exhaustive]`; its `Display` and `source` give the position and the opaque cause.

Not enforced, and left to the implementer as the sealed intent states: that each function has no side effect, does no input or output and gives the same result for the same arguments (a blocking call in a function blocks a thread); that a function can be called by several runs at the same time; that the empty update of a merger shows the merge only through the update of another node, so a late input that comes before that update finds the spent function false; that the internal node meets the obligations of the contract; that the update of `reduce` stays correct on a later state and fits in the size limit of the state (a run that has done all its work then fails with a refused update, decided by the executor); that after a failure, a cancel or a resume all the elements run again with new instance ids, so effects of the internal node outside its state can happen again.

## br-graph-executor

The executor of the graph family: it runs a graph built with br-graph-contract, event-driven, one event handled at a time. It creates the instances of nodes just in time and gives them inputs, applies the update of each `Result` and evaluates the edge, hands the state over to the targets, stops on the first failure (fail fast), cancels with a grace period, and produces checkpoints a host can store and start again from. It owns no node, no definition of state, edge or graph, and no storage. It depends on `br-graph-contract` and not on a library of nodes, and it needs a Tokio runtime with tasks and timers. Not on crates.io; pin the git tag `{crate}/v{version}` with the matching `version` beside it, and the contract on its own tag beside it (a crate that names another crate's types must resolve it through the same git url and tag):

```toml
[dependencies]
br-graph-executor = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-graph-executor", tag = "br-graph-executor/v0.1.0", version = "0.1.0" }
br-graph-contract = { git = "https://github.com/BotResources/br-llm-graph-rs", package = "br-graph-contract", tag = "br-graph-contract/v0.1.0", version = "0.1.0" }
```

| Part | What the crate gives |
|---|---|
| `start`, `resume` | a run from a graph (`Arc<Graph>`), a state and a list of nodes, or from a graph and a `Checkpoint` with the state limit the host gives; `StartError` and `ResumeError` refuse a bad list or a checkpoint that does not fit the graph (the version, the node names and the edges are compared; the entries, kinds and subkinds are not) |
| `RunConfig` | the cancel token (new for each run: the executor triggers it on a failure) and the grace period (one whose deadline cannot be represented, `Duration::MAX` for instance, counts as about 30 years), both required, and the optional `CheckpointSink` and `Observer` |
| `Outcome` | `RunEnded` with the final state; `RunFailed` and `RunCancelled` with the failures in the order handled (the first is the reported cause) and the most current `Checkpoint` |
| `RunFailure` | one named cause per failure, with a reason as text and the opaque cause when a node or an edge gave one; `SinkRefused` carries the checkpoint the sink refused |
| `Checkpoint` | the state, the structure of the graph and the names of the nodes that are not idle; serializable data with one wire form in which the state is a JSON string, exact through every serde door |

Behaviour to know:

- A failure is a stop: the executor triggers the cancel token itself (unless the host did first), holds every new input, gives the live instances the grace period, applies a `Result` that reached it before the grace ended, then drops what is left. A failure detected before the host cancelled gives `RunFailed`; after it, `RunCancelled` with the failures listed. If every node is idle at the end of a step and no failure was recorded, the run has ended. A recorded failure is never lost as `RunEnded`: with every node idle it still gives `RunFailed` (the executor triggered the token) or `RunCancelled` (the host did first), and such an outcome can carry a checkpoint that names no node, which `resume` refuses with `NoNodes`.
- Panics are caught in the creation of an instance, in an edge, in the sink, in the observer, in the apply of the fields (an update refused), in the `Display` of a node, creation or edge error (the panic text becomes the reason), in the serialization of the state when a checkpoint is built (`CheckpointNotBuilt`, whose outcome checkpoint then holds an empty string as its state, which `resume` refuses as `StateRejected`) and in the rebuild of the state on a resume (`ResumeError::StateRejected`). A panic of `Clone` of the fields at the end of a run is not caught. A panic is caught only when unwinding is enabled.
- The sink is called one checkpoint at a time inside the step: the run waits for it, and a cancel is read again when it answers. `Observer::observe` is synchronous and runs in a task of the host's runtime, fed without limit and never awaited by the run; it must return quickly, and an observer that blocks its thread stalls a `current_thread` runtime.
- The public API names the types of br-graph-contract (`Fields`, `State`, `Graph`, `CancelToken`, `Cause`, `InstanceId`, `Target`, `Structure`) and `std::time::Duration`, `std::num::NonZeroUsize` and the serde traits (`Checkpoint` implements them).
- The executor retries nothing, detects no stuck run, stores no checkpoint and starts no run again: those are the host's.

## Releases

Releases: a version ships only once the BotResources libraries registry holds it as a sealed patch. The crates `crates/br-llm-graph`, `crates/br-graph-contract`, `crates/br-flow-nodes` and `crates/br-graph-executor` are tracked, each by its own tag `{crate}/v{version}`. The pull request that bumps a crate's `Cargo.toml` (with a `## X.Y.Z - date` heading in its CHANGELOG) is gated by the registry; on merge to `main` (or a `release/{crate}-<major>.<minor>` line) the release workflow asks the registry again, tags `{crate}/vX.Y.Z`, creates the GitHub release, then records the API listing, the dependencies and the release in the registry.

Coverage: every CI run measures the test coverage with `cargo-llvm-cov` and keeps it as the `coverage` artifact (a `coverage-summary.json` per crate and per file, the lcov file and the HTML report); on a pull request the run's summary compares it with the base branch and warns when line coverage drops. Each release attaches its `coverage-summary.json` to the GitHub release and compares it with the previous release that carries one.

No support is provided; paid support is available on request from BotResources. Apache-2.0. Generated by an AI system (Anthropic Claude) under the direction and review of BotResources, which takes full responsibility for it (EU AI Act, Regulation (EU) 2024/1689).
