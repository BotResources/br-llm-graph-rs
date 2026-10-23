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
| `State` | create, apply (all or nothing) and rebuild check the fields' own validity and a size limit on their JSON form |
| `Instance::channel` | `give` is ordered against the close (accepted before it and then read, or `InputClosed`); exactly one terminal `Event`; a dropped end ends the instance |
| `GraphBuilder`, `Structure` | one set of build rules shared by the build and the rebuild of a structure; a `Graph` or a `Structure` that exists is valid |
| `Graph::evaluate_edge` | targets declared by the edge, no `END` mixed with nodes, no repeats, in declaration order |
| `CancelToken` | never un-triggered; a child is triggered by its parent and never the reverse |

Not enforced, and left to the implementer as the sealed intent states: that serialize then rebuild gives equal fields, that applying an update and validating are deterministic, that an instance reads its accepted inputs before its terminal event and finishes its work before it, that an edge is a pure function of the state, that an instance never changes the state it reads, and that the executor gives each instance a new id and evaluates edges only after a `Result`.

## Releases

Releases: a version ships only once the BotResources libraries registry holds it as a sealed patch. The pull request that bumps `crates/br-llm-graph/Cargo.toml` (with a plain `## X.Y.Z` heading in its CHANGELOG) is gated by the registry; on merge to `main` (or a `release/br-llm-graph-<major>.<minor>` line) the release workflow asks the registry again, tags `br-llm-graph/vX.Y.Z`, creates the GitHub release, then records the API listing, the dependencies and the release in the registry.

Coverage: every CI run measures the test coverage with `cargo-llvm-cov` and keeps it as the `coverage` artifact (a `coverage-summary.json` per crate and per file, the lcov file and the HTML report); on a pull request the run's summary compares it with the base branch and warns when line coverage drops. Each release attaches its `coverage-summary.json` to the GitHub release and compares it with the previous release that carries one.

No support is provided; paid support is available on request from BotResources. Apache-2.0. Generated by an AI system (Anthropic Claude) under the direction and review of BotResources, which takes full responsibility for it (EU AI Act, Regulation (EU) 2024/1689).
