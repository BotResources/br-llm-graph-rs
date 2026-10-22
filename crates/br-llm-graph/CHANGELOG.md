# Changelog

All notable changes to `br-llm-graph` are documented here. The git tag
`br-llm-graph/v{version}` releases the crate; versions up to 0.2.0 were
released under the tag `v{version}`. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow semver.
Release headings are plain `## X.Y.Z` — the release pipeline greps that exact
form to decide whether a version ships.

## Unreleased

## 0.7.0 - 2026-10-22

Breaking: a runner calls another runner, as a node or as a tool of an agent,
through a runner port the host implements; a tool call gets its id, its name
and the origin of its tool node; a call to a tool no tool node runs is
answered, never a run failure; a structured reply is checked against its
shape, and a reply that cannot be used is asked for again within the node's
reply retries; the node finished event says how the node ended; the thread
store is given each finished step, with its model and its usage.

### Added

- The runner call (module `runner`, re-exported at the root). `RunnerPort`,
  the host's port, takes one call (`RunnerRequest`: its `CallKey`, its
  `RunnerTarget`, its arguments, a JSON object) and gives
  `RunnerReply::Answered(RunnerAnswer)` (the child's blocks in order,
  `AnswerBlock::Text` / `Structured`, and its thread),
  `RunnerReply::Failed { reason }`, or a `RunnerError`. The library owns no
  task, timer or transport: a deadline on the answer is the port's, and a
  cancel drops the call, whose child the port cancels once.
- `CallKey` names one runner call across attempts: the origin of the node
  that makes it, its arguments as canonical JSON text (no whitespace, object
  names sorted by their UTF-8 bytes, strings and numbers as serde_json writes
  them, so `1` and `1.0` are two keys) and, for a tool, the tool call id. A
  resumed run gives the keys it gave; a loop that runs a node again with the
  same arguments names the same call. A port hashes the key before deriving
  ids from it.
- `RunnerTarget`, a runner type, refused when blank (`RunnerTargetBlank`).
- `RunnerCall`, a node: its arguments fixed or a function of the state and
  the configuration (`RunnerArguments`); the answer's text (the JSON text of
  its structured block when it has one, else its text blocks joined
  verbatim), and optionally the child's thread, through a `TextOutput`
  (`Set`, `Append`). An expected failure of the child fails it with
  `RunnerFailed { target, reason }`, whose message is the child's reason
  verbatim (`runner call failed` when blank), so a `Capture` turns it into
  data; an error of the port fails it with `Runner`, carrying it. Its outputs
  are checked at build, two `Set` of one key included (`SetConflict`); as a
  map body, `Append` only (`MapBodySet`). Described as `runner_call`: its
  target, how its arguments are given (never their value) and its outputs.
- `RunnerTool`, a tool: its spec the runner's, an optional hook, its
  arguments the tool call's, its key the tool node's origin and the tool call
  id; its result the answer's blocks, a structured value as its JSON text; a
  child's failure is an error result holding the reason, a port error a tool
  error. Described by its name, as any tool.
- `ToolContext`, what a tool call is called with: the call's id and name and
  the origin of the tool node that runs it.
- `NodeOutcome`, how a node of a superstep ended: `Updates` (it returned its
  updates, or reused those it had recorded; applying the superstep may still
  refuse them), `Error` or `Panic`. It follows what the node itself did: a
  call whose child fails returns an error, or its capture updates under
  `OnFailure::Capture`; a panic in a map's body is the map's. Exhaustive, as
  `TurnEnd`.
- A structured value fits its shape when it is an object that holds every
  required parameter and no name the shape does not declare, each value of
  its parameter's kind (a string, a number, a JSON integer, a boolean, a
  string among an enum's values, an array whose items fit, an object that
  fits its parameters); a `null` member holds nothing. The reason names the
  place, never the value found.
- Replies that cannot be used: one the client refuses (`ReplyUnusable`,
  `MalformedResponse`), or, with a structured output mode, a final reply (one
  that calls no tool) whose structured block is missing or does not fit. The
  LLM call, the ReAct agent and the generator seat ask for it again within
  `reply_retries` (`Count`, 0 by default): the request built again for the
  next attempt, then a user message holding `reminder` (a `Prompt`, each
  `{reason}` replaced; one more text of a trailing relay; a blank reminder
  sends the request alone), the attempt one higher. A retry never becomes
  part of the conversation and counts against no budget and no round limit;
  the generator's is no generation, no revision and no critique. With no
  retry left the node fails with `Model` or `Structured`. The round limit's
  `End` closing step is not checked.
- The reply retries keys (int) of the ReAct agent and of the generator; the
  agents' entry refuses a negative one before any model call
  (`CountNegative`).
- `RequestContext::attempt` on every context, counted by the reply retries,
  and for the critic by its verdict retries.
- `Reading`: a verdict reader gives `Reading::Verdict`, or
  `Reading::Unreadable { reason }`, an unreadable verdict asked for again
  with that reason; its error still fails the run as returned.
- `ThreadStore::finish_step`, a provided method that does nothing by
  default: once the stream of a step a client returned has ended, the store
  is given the step, with the model that answered and its usage.
- The description writes `reply_retries` and `reminder` on `llm_call` and on
  the ReAct agent's and the generator's `model_call`.
- The example `runner_call`, with an in-memory runner port a host can copy
  (`examples/common/runners.rs`).

### Changed

- **Breaking:** `Tool::call` takes `&ToolContext` before its arguments.
- **Breaking:** a pending call that names a tool no tool node runs is not
  executed: the first tool node answers it with `Tool {name} is not
  available: this call was not executed. The tools available are: {names}.`
  (the declared tools the enabled key holds, in declaration order), or
  `… No tool is available.` when none is offered, and the turn goes on. Past
  the last allowed round the round limit still answers first.
- **Breaking:** without a split, a ReAct agent or a generator seat always has
  its node `tools`, even without a tool; its description holds it and the
  model call's edge declares it.
- **Breaking:** `Observer::node_finished` takes the node's `NodeOutcome`.
- **Breaking:** `LlmCall`, `ReactAgent` and `GeneratorSeat` gain
  `reply_retries` and `reminder`.
- **Breaking:** `LlmOutput` and `RunnerOutput` are one type, `TextOutput`.
- **Breaking:** `VerdictReader` returns `Result<Reading, GraphError>`.
- **Breaking:** a structured reply that does not fit its shape, or has no
  structured block, fails with `Structured` (the LLM call wrote its text
  blocks in 0.6).
- **Breaking:** a critic reply the client refuses as unusable or malformed
  is an unreadable verdict, asked for again within the verdict retries, then
  `Structured` (it failed with `Model` in 0.6).
- **Breaking:** a reply asked for again asks the model again, the attempt
  one higher, and the client it gives runs the rest of the turn (0.6 ran a
  turn on one client).
- **Breaking:** a durable generator-critic's turn of the generator that the
  thread holds failed or cancelled is no generation: it stays in the
  conversation, is neither judged nor counted, and the call is routed on the
  turn before it (0.6 judged a generation a crash cut, on its partial text).
- A durable agent with a structured output mode pushes a step's events to
  its thread once the step is checked: it no longer streams into its thread
  live. In the plain mode, a reply refused after some of its events reached
  the thread, with a retry left, fails the turn (`TurnEnd::Failed`), and the
  agent reads its thread again and asks in the turn of the next rank.
  `ThreadStore` documents the ends the library sends.
- **Breaking:** `GraphError` gains `RunnerFailed`, `Runner` and
  `RunnerTargetBlank`; `runner_call` is a library kind, so
  `NodeDescription::labelled("runner_call")` is refused.

### Removed

- **Breaking:** `GraphError::PendingToolUnsatisfiable`.

## 0.6.0 - 2026-10-16

Breaking: a built graph describes itself as data (`Graph::describe`), so a
host can compare the graph it built with the graph it meant to build; edges
may declare their targets, which the build and the run then hold them to.

### Added

- `Graph::describe` gives a `GraphDescription`: the graph's entry, its
  signature, its schema (state and configuration keys with their kinds,
  and the keys that have a default, by kind; an agent's private keys are
  not part of it) and its nodes in
  registration order, each with its id, its kind, its realizes labels, its
  edge's declared targets in the order given (or undeclared) and the
  fields of its kind, nested graphs included. It runs nothing, calls no
  client, reads no state and no configuration; two builds from the same
  parts give equal descriptions, in the same order.
- A description writes no state value, default, prompt, client, author
  name or secret; it does write fixed bounds (a fixed `Limit` or `Count`,
  the generator-critic's `final_generation`). The schema is `{"state", "config",
  "defaults"?}`, `defaults` mapping each state key that has a default to
  its kind, never the default's value, whatever its kind (a string, a
  list, a conversation). An author (of a model-calling node, of an LLM
  call's conversation request) is written by its id, `{"id"}`, never its
  name, which is rendered into what a model reads.
- Its JSON, `br-llm-graph-description/1` (`DESCRIPTION_VERSION`):
  `{"description_version", "entry", "signature", "schema", "nodes"}`; a
  node `{"id", "kind", "realizes"?, "join"?, "edge", ...}`; an edge
  `{"targets": [{"node"} | {"end"}]}` or `{"undeclared": true}`; a nested
  graph without its version. Written only: `GraphDescription` and
  `NodeDescription` implement `Serialize` and not `Deserialize` (pinned by
  `compile_fail` doctests).
- The kinds: `node` and `join` (a host's node, by default), `call`,
  `react_agent` and `generator_critic` (a call of an agent's graph, with
  everything a call holds), `map`, `llm_call`, `model_call` (an agent's
  model-calling step and the critic's call) and `tools` (a tool node). A
  node of another kind registered as a join carries `"join": true`.
- What each kind holds:
  - a call: its inputs, configuration keys and outputs with their sources
    and targets, its end label, its failure policy with its captures, and
    the description of its graph; a fixed value is written as its kind
    (`{"const": kind}`), never the value;
  - a map: its list and item keys, its concurrency limit, its item-failure
    policy and its body, a node without id or edge;
  - an LLM call: how its model and system are given, its request and its
    output;
  - a model-calling node: its author's id, how its model, system and history
    are given (`fixed`, `ready_made` or `function`, never the value), its
    tool names, its enabled key, its tool concurrency and round limit. The
    critic's node adds the critique budget, `final_generation`, the verdict
    retries, how the review request is given and whether the verdict shape
    is the default; the generator-critic's `check` holds the revision limit
    and how the rule check and the empty-answer critique are given;
  - a tool node: the names of its tools.
  A `Limit` or a `Count` is `{"fixed": n}` or `{"config": key}`.
- `Node::describe`, a provided method: what a node says of itself
  (`NodeDescription`), by default `node` or `join` with nothing nested. A
  host's node may give its own kind label (`NodeDescription::labelled`; a
  label that is not an identifier or that names a library kind is refused,
  `Identifier` with the field `kind_label`) and a nested description
  (`NodeDescription::nesting`). A node that wraps another forwards it; the
  generator-critic's `prepare` does.
- Realizes labels: `Node::realizing(labels)` wraps any node in
  `Realizing<N>`, which runs, checks and describes as its node does and
  adds the labels, free text in the order given; the library never reads
  them. `NodeDescription::realizing` adds labels to a description.
- `Edge::declared`, a provided method: the targets an edge may give, known
  without running it; `None` by default (undeclared). `Always` declares its
  target; `FnEdge::targets` declares an edge built from a function, its list
  kept as given. The graph asks for the declared targets once, at build,
  and keeps them in the order given, a repeated target once at its first
  place: the run checks that copy and the description writes it.
- `GraphError::UndeclaredTarget { node, target }`: a declared edge that
  returns a target it did not declare fails the run, the run's error
  itself, before any target runs. Per edge, in registration order: the
  edge's error, `EmptyEdge`, `UndeclaredTarget`, then `UnknownNode`.
- `GraphError::EdgeToUnknownNode { node, target }`: the build refuses a
  declared node target the graph does not hold, checked last, after the
  node checks; undeclared targets and end labels are not checked.
- The library's own graphs declare every edge's targets: the ReAct agent
  (`enter`, the model call with its tool nodes, its round-limit node and
  the answer, the tool nodes, the round-limit node), the generator-critic
  (`route`, `check`, `critique`, `finish` with its labels `validated` and
  `exhausted`, `prepare`).
- `Target` is displayed (`node {id}`, `end {label}`) and serialized
  (`{"node": id}`, `{"end": label}`). The two new refusals read alike: "the
  edge of node a declares node ghost, which is not in the graph", "the edge
  of node a returned node ghost, which it does not declare".
- The public module `describe` holds `GraphDescription`, `NodeDescription`,
  `Realizing` and `DESCRIPTION_VERSION`, all re-exported at the root.
  `GraphDescription` derives `Debug`, `Clone` and `PartialEq`;
  `NodeDescription` derives `Debug`, `Clone`, `PartialEq` and `Default` (the
  description of a node without a kind of its own). `Realizing::labels`
  gives a wrapped node's labels, `Realizing::node` the node it carries. The
  rustdoc of `GraphDescription` tables the fields of each kind.
- The example `describe` prints the description of a graph with declared
  edges, a call, a map and an agent.

### Changed

- **Breaking:** an `Always` (or a declared `FnEdge`) naming a node the
  graph does not hold is refused at build with `EdgeToUnknownNode`; 0.5
  built it and failed the run only when the edge ran (`UnknownNode`).
- **Breaking:** `GraphError` gains `UndeclaredTarget` and
  `EdgeToUnknownNode`; a `match` on it without a wildcard arm no longer
  compiles.
- **Breaking:** `Edge` gains the provided method `declared` and `Node` the
  provided methods `describe` and `realizing`: a host type that implements
  them and also has inherent or trait methods of these names may need to
  disambiguate its calls.
- `GraphBuilder::build` documents and applies the new refusal order:
  `EdgeToUnknownNode` after `InvalidNode`.

## 0.5.0 - 2026-10-16

Breaking: an agent works in a thread of a thread store the host gives, and
resumes from it; without one it works in memory, for the current call only.
A runner gives a model-calling node its model, system and history, each a
value or a function of the node's context, and reads and writes the state
through typed keys. The graph builds on br-llm-messages 0.5 and
br-llm-clients-core 0.4.

### Added

- Typed keys. Declaring a key on a `SchemaBuilder` gives a `StateKey<T>`
  (`state`, `state_with_default`) or a `ConfigKey<T>` (`config`), `T` the
  Rust type of its kind (`i64`, `f64`, `String`, `bool`, `Vec<T>`,
  `Conversation`), mapped by the sealed trait `ValueType`: no other type can
  be declared. `State::get` and `Config::get` read through a typed key;
  `StateKey::set` takes only a value of its type, `append` only an element,
  and `input`, `push_turn`, `push_step` and `push_result` exist only on a
  conversation key; `given` gives a start input. A call maps typed keys of
  one kind (`SubGraph::input_from`, `input_from_config`, `input_value`,
  `config_from`, `config_from_state`, `config_value`, `output_set`,
  `output_append`, `end_label_set`, `end_label_append`), so does a capture
  (`CaptureUpdate::set_value`, `set_from`, `set_reason`, `append_value`,
  `append_from`, `append_reason`) and a map (`Map::new(list, item, body)`,
  `max_concurrency`, `on_item_failure`); `Limit::config` and `Count::config`
  take an int configuration key. A wrong kind does not compile (pinned by
  `compile_fail` doctests, which CI runs with `cargo test --doc`); a float is still checked when built
  (`FloatNotFinite`). Reading and updating by name stay for states,
  configurations and checkpoints reloaded from JSON and for graphs described
  as data, with their refusals.
- What a runner gives a model-calling node: `ModelNode`, the sealed trait of
  the nodes that call a model (`LlmCall`, `ReactAgent`, `GeneratorSeat`,
  `CriticSeat`, and `GeneratorCritic` for its rule check and empty-answer
  critique), and `Given<N, T>`, a value
  (`Given::value`) or a synchronous function of the node's context
  (`Given::from_fn`) declaring the keys it reads (`Reads`); `Model<N>` gives
  a client, `Prompt<N>` a text, `History<N>` the wire messages. The context,
  `RequestContext<N>`, gives the state and the configuration through the
  declared typed keys only (`UnknownKey` otherwise), and what the node knows:
  an agent's conversation (`AgentNode`), the round of the ReAct loop
  (`LoopNode`), the generation number and the budget (`GeneratorCriticNode`),
  the critic's answer and attempt, the generation judged (`answer_step`).
  Ready-made values: `Prompt::text`, `from_state`, `from_state_list`,
  `from_config`, `from_config_list` (a list of strings joined by a blank
  line), `Model::client`, `Model::schedule` (from the start, the last
  repeating; `Scheduled`) and `Model::ending_with` (a tail for the last calls
  the budget allows; `Budgeted`, the generator-critic's seats only; the
  generator's calls allowed, the budget plus one with `final_generation`,
  saturate at `usize::MAX`). A
  function's error fails its node as returned. The keys a function of an
  agent declares are inputs and configuration keys of the agent's graph; a
  key given twice with one kind is declared once.
- `ThreadStore`, the host's port through which a durable agent reads and
  writes its thread (`read`, `create`, `store_input`, `open_turn`,
  `push_event`, `push_results`, `end_turn`), with `ThreadView`,
  `ThreadMessage`, `MessageStatus`, `TurnEnd`, `ThreadError` and
  `ThreadFuture`; its documentation states how each write is repeated
  safely and how a failed or cancelled turn is repaired, and which ends the
  library sends: `Completed`, and `Failed` only for a turn found open on
  entry (a turn left by a model error, a store error or a cancel stays open
  until then). `ThreadView` and `ThreadMessage` are `#[non_exhaustive]`
  structs a store builds with `new`; `MessageStatus` and `TurnEnd` are
  exhaustive on purpose, so a store matching them must handle a status or
  an end added later.
- `ThreadId`, a UUIDv7 written as its text, and `IdRequest`, what an id is
  asked for: the origin of the agent, its trigger (the id of the user input
  its call answers) and the rank of the id within the call, a
  `#[non_exhaustive]` struct built with `IdRequest::new`.
- Durable agents: `ReactAgent::thread_store` and
  `GeneratorCritic::thread_store`. A durable agent takes the input `thread`
  (an id the store holds, or the empty string to have the store create one
  under an id from the id source) and gives it back as the output `thread`;
  its `opening` holds user inputs ending with the trigger, each stored unless
  the thread holds its id. On entry it reads its thread once: a turn of the
  call found open is failed (and repaired by the store), a completed one
  that ends the call gives the outputs without a model call, otherwise the
  call goes on in the turn of the next rank. Every step, tool results item
  and turn end is written to the thread first; the model's stream events go
  into the open turn.
- `GeneratorCritic`: the rule check (`RuleCheck`, `Ruling::Pass`,
  `Ruling::Revise(text)`), recorded as a revision (`other(rule_check)`)
  while the revisions in a row are below `revision_limit` (`Count`, default
  2); `verdict_retries` (`Count`, default 0), asking an unreadable verdict
  again with `Your reply could not be read as a verdict: {reason}. Reply
  again with the verdict.`; the acceptance it records (`other(accepted)`,
  `Accepted.` when the message is blank); the critic's defaults
  `CriticHistory::History`, `Latest` and `Brief`, each ending with the
  wrapped answer and keeping a stable beginning for the provider's cache.
- `LlmRequest::History`, the conversation to send as wire messages, and
  `LlmOutput` (`Set` a string key, `Append` to a list of strings).
- `GraphError::Thread` (the store's error, its source), `ThreadNotFound`,
  `ThreadId` and `OpeningNotInputs`. There is no `From<ThreadError>`:
  `ThreadError` is any boxed error, so the conversion would have turned the
  boxed error of a node, a tool or a hook into `Thread`.
- `LlmOutput`, `Ruling` and `CriticHistory` are `#[non_exhaustive]`, as
  `Judgment` and `LlmRequest`: a `match` on them outside the crate needs a
  wildcard arm.
- `#![warn(missing_docs)]`: every public item is documented, `GraphError`
  and `NodeFault` variants and fields included; crate docs with a runnable
  example over typed keys.
- Examples on the 0.5 API (each runs with `cargo run -p br-llm-graph
  --example <name>`), durable agents among them, over a thread store kept in
  memory in `examples/common`.

### Changed

- **Breaking:** br-llm-messages 0.5 (tag `br-llm-messages/v0.5.0`): every
  user input carries an id chosen by its creator, and input ids and turn ids
  are unique together in a conversation. An `Update::Input` or a turn whose
  id the conversation already holds is refused as `Message`
  (`DuplicateEntryId`). br-llm-clients-core 0.4 and br-llm-clients-mock 0.4
  (tag `br-llm-clients-core/v0.4.0` for both) move to it.
- **Breaking:** `Inbox`, `Sender` and `channel()` are renamed `Control`,
  `ControlSender` and `control()`: the control channel.
- **Breaking:** the by-name schema declarations are
  `SchemaBuilder::state_by_name`, `state_by_name_with_default` and
  `config_by_name` (formerly `state`, `state_with_default` and `config`,
  which now declare typed keys).
- **Breaking:** `IdSource` gives `thread_id(&IdRequest) -> ThreadId` and
  `turn_id(&IdRequest) -> TurnId` (formerly `turn_id()`): the same id for the
  same request, so a resumed run, which keeps its run id, receives the ids it
  had. The id of an agent's turn is also the id of the thread message that
  holds it.
- **Breaking:** `LlmCall { model, system, request, output_mode, output }`:
  `model` (formerly `client`) is a `Model<LlmCall>`, `system` a
  `Prompt<LlmCall>`, `request` a `LlmRequest` (`Text(Prompt)`,
  `Conversation { key: StateKey<Conversation>, author }`, `History(History)`)
  and `output` a `LlmOutput` (formerly `Output`). Its build checks the keys
  its functions declare against its graph (`UnknownKey`, `KindMismatch`). Its
  stream events carry the output key for any request that is not a
  conversation key.
- **Breaking:** `ReactAgent { author, model, system, history, output_mode,
  tools, tool_nodes, tool_concurrency, round_limit, enabled, inputs,
  thread_store }`: `model` (formerly `client`), `system` a `Prompt` (formerly
  a list of `Source`), `history` an optional `History` (by default the
  agent's conversation rendered for its author). The input and output
  `history` gives way to the input `opening`, durable the input and output
  `thread`, and the private key `conversation`, neither input nor output;
  the graph enters through the new node `enter`. `enter`, `opening`,
  `conversation`, its private key `turn_rank` and, durable, `thread` are
  the graph's own (`DuplicateNode`, `KeyCollision`: a function, an enabled
  key or an extra input naming `turn_rank` is refused too); a round-limit
  flag naming `answer`,
  `opening`, `thread` or `conversation` is refused (`FlagKeyMismatch`).
- **Breaking:** an agent's model gives the client of a turn, asked at the
  turn's first round and kept for its other rounds, so a turn is run by one
  client; a schedule advances per turn. The client is kept in the run of
  the agent's call, for its turn: two runs of one built graph, whatever
  their run ids, and the items of a map each keep their own, and nothing is
  kept once a run ends, however it ends (finished, failed, cancelled, or
  paused and never resumed). A call run again asks its model again at its
  first round.
- **Breaking:** `GeneratorCritic`: the inputs `conversation` and
  `generator_history` give way to `opening`, durable `thread` (also an
  output), and the private key `conversation`, which holds the opening,
  every generation with its tool traffic and thinking, and every message of
  the critic, and is the thread when durable. The node `publish` goes and
  `check` comes: an empty answer is rejected without the critic, then the
  rule check judges each generation. The budget (`max_critiques`) counts
  critiques only, revisions not; `nb_generations` counts the generator's
  turns of the call, revisions included. A turn of the call found on entry
  is routed as it would have been: a revision sends the generator back
  whatever the budget, an acceptance finishes `validated` without a model
  call. A turn found open on entry is repaired, then routed by its kind: a
  repaired generation goes to the check, which may judge it empty or pass
  it on its partial text; a message of the critic is a judgment only with
  its exact stop reason (`end_turn`, `other(rule_check)`,
  `other(accepted)`), so a repaired one is no judgment: the critic judges
  the same answer again, and it is not counted against the budget.
  `GeneratorSeat { model, system, history, .. }` and `CriticSeat {
  model, system, history, review_request, thinking_description,
  is_valid_description, message_description, verdict }`, the texts
  `Prompt`s of their seat, `empty_answer_critique` a
  `Prompt<GeneratorCritic>`; `CriticSeat::new(author, model, system)`.
  `route`, `generate`, `check`, `critique`, `finish`, `prepare`, `opening`,
  `answer`, `is_validated`, `nb_generations`, `conversation`, the private
  keys `turn_rank` and `call_turns` and, durable, `thread` are the graph's
  own (`DuplicateNode`, `KeyCollision`). The
  critic's stream events carry the key `critic`. Each seat's schedule counts
  critiques (revisions and verdict retries not counted).
- **Breaking:** a `Count` or a `Limit` an agent reads from its configuration
  is resolved by the edge of its entry node before any node reads it: a
  refused one (`CountNegative`, `LimitNotPositive`, or its key's
  `UnknownKey` or `KindMismatch`) is the run's error itself, before any
  model is called. The entry node runs first: a durable agent has already
  created its thread (when given the empty `thread`) and stored the inputs
  of its `opening`, and a generator-critic's `prepare` has run, when the
  edge refuses.
- **Breaking:** `GraphBuilder::input` and `output` take `impl Into<Key>`, a
  typed key (`&StateKey<T>`) or a key by name, formerly a `Key`: a call
  passing `x.into()` no longer infers the type to convert into; pass the
  key itself.
- **Breaking:** `Graph::schema` does not declare an agent's private keys;
  a checkpoint of an agent's graph, its pending entries and the observer's
  `checkpoint` events omit `conversation`: the checkpoint's cursor names the
  graph's entry and holds no pending entry, so a run of that graph resumed
  from it enters again through the entry, where a durable agent rehydrates
  from its thread and an agent in memory starts again from its opening.
- **Breaking (stored data):** states whose conversations carry the schema
  `br-llm-messages/2` no longer load (br-llm-messages 0.5 writes `/3`); the
  graph's own schema stays `br-llm-graph/1`. A state or a checkpoint of a
  0.4 agent's graph (its `history`, `conversation` or `generator_history`
  keys) is refused against the 0.5 graph (`StateSchemaMismatch`, or the
  refusals of a reload): a durable agent's resumption is its thread.

### Removed

- **Breaking:** `Source`, `SourceFn` and `GraphError::SourceKeyMismatch`: a
  model-calling node takes a `Model`, a `Prompt` and a `History`.
- **Breaking:** `Inbox`, `Sender` and `channel` (see `Control`).
- **Breaking:** the `ReactAgent` input and output `history`, and the
  `GeneratorCritic` inputs `conversation` and `generator_history` and its
  node `publish`.

### Fixed

- A round limit or a tool concurrency an agent reads from its configuration
  is refused even when the run never reaches the node that reads it: in 0.4
  a round limit below one was never refused when the model answered without
  calling a tool, and a tool concurrency below one only once a tool node ran,
  after the model had been called.

## 0.4.0 - 2026-10-15

Breaking: the model is a `Client` of br-llm-clients-core 0.3, and the graph
builds on br-llm-messages 0.4. The graph's own model port, the thinking
switches and the safe flag of a tool are gone; a tool may carry a hook
instead, and `LlmCall` makes one model call whose answer is data.

### Added

- Dependency on br-llm-clients-core 0.3 (git tag
  `br-llm-clients-core/v0.3.0`). The tests and the examples run on
  br-llm-clients-mock 0.3, a development dependency pinned on that same tag:
  nothing of it appears in the public API.
- `LlmCall { client, system, request, output_mode, output }`: a node that
  makes one model call and gives back its answer as data. `request` is an
  `LlmRequest`: `Text(Source)`, sent as one user message holding its text (a
  text that resolves blank fails the node with `Message` before any call), or
  `Conversation { key, author }`, the conversation key rendered for `author`
  (for images or earlier turns). The system text is the text of `system`; a
  blank text gives no system text. The request declares no tools and sends
  `output_mode`. The answer, the JSON text of the reply's structured block
  when it has one, else its text blocks joined verbatim, is written through
  `output` (`Output::Set` to a string state key, `Output::Append` to a list of
  strings); nothing else is written, the call keeps no conversation. Its
  stream events reach the observer with the conversation key, or the output
  key for a text request. As the body of a `Map`, an `Append` output makes one
  model call per item, at most the map's concurrency limit at once, the
  answers appended in item order; a `Set` output is refused there
  (`MapBodySet`). At build it checks the keys its sources read
  (`SourceKeyMismatch`), its conversation key (`UnknownKey`, `KindMismatch`)
  and its output key, refused with the error writing a string through it
  would give (`UnknownKey`, `KindMismatch`, `AppendNotList`). A call can not
  have tools; it may ask for a structured reply. Tool calls and results in
  the history of a conversation request reach the client with no tool
  declared, which some providers (Anthropic, Bedrock) refuse: keep tool
  traffic out of an `LlmCall`'s conversation.
- `Hook` (`judge(id, arguments, state, config) -> HookFuture`), `Judgment`
  (`Accept`, `Refuse { reason }`), `HookFuture` and `HookError`, and
  `Tool::hook()` (`Option<&dyn Hook>`, `None` by default): the judgment of
  one tool call before its tool runs. A call is judged after the `enabled`
  key check (a call naming a tool that is not enabled is answered without its
  hook), inside its slot of the tool concurrency limit. A refused call is not
  executed: it is answered with an error result holding the reason (`tool
  call refused` when blank), and the turn goes on. An error of a hook fails
  the tool node and that call is not executed: no call of the node that has
  not started is started afterwards, the calls already started finish, the
  node's results are dropped, and the error returned, as the hook returned
  it, is the first in call order of the hooks that failed. A failed node
  records nothing, so a resumed run judges and runs its calls again, under
  the same call ids: tool effects are at-least-once, and an approval hook
  memoises by call id. A hook returns no update. Hooks are the host's: the
  graph ships none; `Hook`'s documentation lists what a hook must allow for
  (an error is not a refusal, cancel-safety, the slot a judgment holds).
- `Judgment` and `LlmRequest` are `#[non_exhaustive]`: a host builds their
  variants, and a `match` on them outside the crate needs a wildcard arm.
- `GraphError::Request(BuildError)`: br-llm-clients-core refused to build the
  request of a model call; the `BuildError` is the error's source.
- `GraphError::VerdictShapeMismatch { reason }`, which replaces
  `VerdictSchemaMismatch`: an override without a reader whose shape does not
  hold `is_valid` (boolean) and `message` (string), both required.
- `From<ClientError>` and `From<BuildError>` for `GraphError` (`Model`,
  `Request`).
- `Output` (of `SubGraph`) is also where an `LlmCall` writes its answer.
- Examples `llm_call` (an `LlmCall` as a map body, then one over the
  results) and `tool_hook` (a host-written hook guarding `write_file`).

### Changed

- **Breaking:** br-llm-messages 0.4 (tag `br-llm-messages/v0.4.0`): an
  `Author` is an id and a name, and the stricter rules of 0.4 on tool names,
  tool call ids and tool results apply to every conversation of a state.
  Stored states whose conversations carry the schema `br-llm-messages/1` no
  longer load. The display of `Update::PushTurn` names the turn by its id (a
  turn no longer displays itself).
- **Breaking:** a model is a `br_llm_clients_core::Client`, given by the
  host: `ReactAgent::client`, `GeneratorSeat::client` and
  `CriticSeat::client` (formerly `model`), and `CriticSeat::new(author,
  client, system)`. The settings of a client, its level of reasoning
  included, are fixed when the host builds it; the graph never sets them and
  reads only the `ReasoningLevel` of the critic's client.
- **Breaking:** `complete(client, &request, ctx, key)` calls a `&dyn Client`
  with a `&ClientRequest` and a sink that sends each stream event to the
  observer, under the calling node's occurrence and the conversation key
  `key`; the call's report is not kept. A model call sends the system text,
  the wire messages, the declared tools and the output mode; a refused request
  fails the node with `Request`, an error of the client with `Model`.
- **Breaking:** `GraphError::Model` carries the `ClientError`
  (`Model(ClientError)`, formerly `Model { message }`), its source.
- **Breaking:** `Tool::spec` gives a `&br_llm_clients_core::ToolSpec` (the
  typed tool vocabulary of core: a name, a description and parameters).
- **Breaking:** `ReactAgent::output_mode` and `GeneratorSeat::output_mode`
  (formerly `output`, named like `LlmCall::output_mode`) are a core
  `OutputMode` (`Plain`, `Structured(StructuredShape)`), formerly the graph's
  `OutputMode::Text` / `Structured { schema }`.
- **Breaking:** `OnLimit::Continue` calls the model once more with its tools
  still declared (core has no setting that forbids tool calls); a reply that
  still calls tools fails the run with `ToolLimitReached`, as before.
- **Breaking:** the critic's default verdict is the core structured shape
  `verdict` with the parameters `thinking` (string), `is_valid` (boolean) and
  `message` (string), in that order, all required. `thinking` is present
  exactly when the `ReasoningLevel` of the critic's client is `Off`: it gives
  a critic that does not reason natively room to reason before judging. Its
  description, when the shape has no `thinking`, is not in use: a state or
  configuration key it reads is not declared. `VerdictOverride { shape,
  reader }` sends the caller's `StructuredShape` (formerly a JSON `schema`).
- **Breaking:** two declared tools with one name, a request no client
  accepts, are refused at build before the rules on tool nodes: an
  `InvalidNode` naming the model-call step (`llm` of a `ReactAgent`,
  `generate` of a generator seat), its reason `Request` carrying
  `DuplicateToolName`. In 0.3.0 they gave `ToolCoveredTwice`, or, with a
  split whose tool nodes cover them by name, built and sent both tools to the
  model.
- A system text that resolves blank (empty or whitespace only) is sent as no
  system text by the model-call step of the ReAct loop (a `ReactAgent`, the
  generator seat) and by `LlmCall`: a core system prompt is a `Text`, which
  refuses a blank text.
- **Breaking:** the critic's system text is always sent: a critic system
  source that resolves to a blank text fails the critique node with
  `Message` (`Blank { field: "text" }`), before the critic's call. A seat of
  each kind may read one source key; it is declared once, and a blank value
  then gives the generator no system text and fails the critic.
- With a conforming client, a reply that holds nothing but thinking is
  refused by the client (`ReplyUnusable`) and fails the generation with
  `Model`; a critic reply without a structured block is refused by the client
  (`MalformedResponse`) and fails the critique with `Model`. The graph's
  empty-answer and `Structured` rules are unchanged and apply to the steps a
  client gives.
- The examples run offline on the scripted mock of br-llm-clients-mock, the
  stand-in for a provider library's client: `react_agent` and
  `generator_critic` lose their thinking switches (the critic's client
  reasons at `Off`, so its verdict carries `thinking`), `external_message`
  decorates a client, `skills` reads the requests back from the mock's log,
  and `react_goal_loop` builds its judge's `ClientRequest` with a structured
  shape.
- Without a split, `tools` is the graph's own node of a `ReactAgent` and of a
  generator seat even when no tool is declared: a round-limit node named
  `tools` is refused there too (`DuplicateNode`).
- A `ReactAgent` flag that names `history` or `answer` is refused
  (`FlagKeyMismatch`) before the rules of the ReAct loop.
- `ToolCoveredTwice` says what it refuses whether one tool node lists a tool
  twice or two nodes run it.
- The crate description names br-llm-clients-core.

### Removed

- **Breaking:** the graph's model port: `Model`, `ModelFuture`, `ModelError`,
  `Request`, `ToolSpec`, `OutputMode`, `ToolCalls` (`Allowed`, `Forbidden`)
  and `StreamSink`. Use those of br-llm-clients-core: `Client`,
  `ClientRequest`, `ToolSpec`, `OutputMode` and `StructuredShape`,
  `StreamSink`, `ClientError`, `BuildError`.
- **Breaking:** `Switch` and `GraphError::SwitchKeyMismatch`, with the
  thinking switch of the model-call step, of `ReactAgent` and of both seats.
  A node or a seat that needs another model or another level of reasoning is
  given another client.
- **Breaking:** `CriticSeat::reasoning_field`: the `thinking` parameter of the
  verdict follows the level of reasoning of the critic's client.
- **Breaking:** `GraphError::VerdictSchemaMismatch` (see
  `VerdictShapeMismatch`).
- **Breaking:** `Tool::safe` and `pending_unsafe_calls`: the optional hook
  replaces them.
- **Breaking:** `ToolNode` is no longer public: it is the tool execution of
  the ReAct loop inside `ReactAgent` and `GeneratorCritic`. `Tool`, `Source`,
  `RoundLimit` and `OnLimit` stay public.
- **Breaking:** `LlmNode` is no longer public: it is the model-call step of
  the ReAct loop inside `ReactAgent` and `GeneratorCritic`. `Source` and
  `SourceFn` stay public.

### Fixed

- The tool concurrency gate keeps one entry per waiting call, in arrival
  order, removed when the call is dropped, and lets one waiter through per
  freed place. Before, every poll of every waiting call pushed a clone of its
  waker, kept until a place freed: a busy call holding the only slot made
  the list grow without bound (40,003 clones for twenty waiting calls, now
  at most one each), and every waiter was woken at each release.
- A round limit read lower than the rounds a resumed turn already holds
  applies as its `OnLimit` says: a reply that asks for tools on such a turn
  comes after the last allowed round, so `End` closes the turn and
  `Continue` answers the calls and calls the model once more, failing only
  when a reply still calls tools after its node has answered in this turn:
  a round of the open turn past the limit (0-based index `max_rounds` or
  more) whose results are all its refusal (error results holding exactly its
  text). `OnLimit::Continue` documents the rule and its residual. Before,
  both failed the run with `ToolLimitReached` unless the rounds equalled the
  limit.
- A generation is taken for one the round limit cut only when the
  generator's round limit is `End` and its final step is the one the limit
  node writes: the stop reason `other(tool_round_limit)`, no model, no usage.
  Before, the stop reason alone decided, and br-llm-clients-core keeps an
  unknown provider stop reason verbatim in `other(..)`: a real answer
  carrying it was judged empty, without the critic, even with no round
  limit at all.

## 0.3.0 - 2026-10-13

### Added

- `Limit`: a positive bound, given as a fixed number or read from an int
  configuration key. A key that is not declared as an int configuration key is
  refused at build (`LimitKeyMismatch`); a value below one is refused at run
  time (`LimitNotPositive`).
- `Switch`: an on/off setting, given as a fixed value or read from a bool
  configuration key, so the host decides at run time. A key that is not
  declared as a bool configuration key is refused at build
  (`SwitchKeyMismatch`).
- `Count`: a bound that may be zero, given as a fixed number or read from an
  int configuration key. A key that is not declared as an int configuration
  key is refused at build (`CountKeyMismatch`); a negative value is refused at
  run time (`CountNegative`).
- `Switch::resolve`, `Limit::resolve` and `Count::resolve` are public, so a
  node of the caller reads the same configuration as the graph, and so are
  `Switch::check`, `Limit::check` and `Count::check`, so that node checks the
  same keys from its `Node::check` and the graph refuses a wrong key at build
  (`SwitchKeyMismatch`, `LimitKeyMismatch`, `CountKeyMismatch`).
- `Request::thinking` (`Option<bool>`): whether the model thinks natively
  before replying, `None` leaving the provider's default. Each model adapter
  translates it for its provider. `LlmNode::thinking` (`Option<Switch>`) is
  resolved at each call and sent on the request; its key is checked at build
  through `Node::check`.
- `Source::Const(String)`, a text fixed at build, and `Source::Fn(SourceFn)`,
  a text computed at each model call from the state and the configuration
  (`SourceFn` is `Arc<dyn Fn(&State, &Config) -> Result<String, GraphError> +
  Send + Sync>`). The function is pure and synchronous: no I/O, anything
  fetched belongs in a node that writes it to the state; it reads only keys
  the graph declares otherwise. `Source::resolve(&State, &Config)` gives the
  text of any source (a list of strings joined by a blank line); `LlmNode`
  reads its `system` through it. `Config(Key)` and `State(Key)` are unchanged
  at run time; at build, `LlmNode` refuses a `State` key that is not a state
  key, or a `Config` key that is not a configuration key, holding a string or
  a list of strings (`SourceKeyMismatch`).
- The `Model` contract, documented on the trait: for `OutputMode::Structured`,
  `complete` returns a step whose structured block is valid against the
  schema, or an error. Validation and retries belong to the model adapter; the
  graph reads the block as given.
- `Map::max_concurrency`: at most that many bodies run at once; the results
  keep the item order. `None` runs every item at once, as before.
- `ToolNode::enabled` (`Option<Key>`, set by `ReactAgent` and
  `GeneratorSeat` from their `enabled`): a pending call naming a tool of the
  node that the key does not hold is not executed; it is answered with an
  error result saying so, and the turn goes on. An `enabled` key of `LlmNode` or `ToolNode` that
  is not a list-of-strings state key is refused at build
  (`EnabledKeyMismatch`).
- `ToolNode::max_concurrency`: at most that many pending calls of the node
  run at once; the results keep the call order. `tool_concurrency` (on
  `ReactAgent` and `GeneratorSeat`): one limit shared by all the tool nodes
  of the agent, at most that many calls at once across all of them (not that
  many per node). `None` runs every call at once, as before. A key that is not an int configuration key is refused at build
  (`LimitKeyMismatch`), by the agent and by `ToolNode::check` alike.
- `round_limit` on `ReactAgent` and `GeneratorSeat` (`RoundLimit`,
  `OnLimit`): bounds the tool rounds of one agent turn. A round is one reply
  that calls tools plus the execution of all its calls; the count starts again
  with each new turn of the agent. When a reply asks for tools after the last
  allowed round:
  - `OnLimit::Error` fails the run with `ToolLimitReached`;
  - `OnLimit::Continue { node, flag }` answers each call with an error result
    saying it was not executed, sets `flag` to true, and calls the model once
    more with tool calls forbidden; a reply that still calls tools fails the
    run with `ToolLimitReached`;
  - `OnLimit::End { node, flag }` answers each call the same way, sets `flag`
    to true, and closes the turn with a step whose stop reason is
    `other(tool_round_limit)`, without calling the model; the agent goes on
    as after a reply that calls no tool.

  `node` names the node the agent adds to do this. A `flag` that is not a bool
  state key is refused at build (`FlagKeyMismatch`).
- `Request::tool_calls` (`ToolCalls::Allowed`, `ToolCalls::Forbidden`): whether
  the model may call tools in this reply. The tools stay declared either way; a
  model adapter maps `Forbidden` to the provider's setting that disables tool
  calls.
- `std::error::Error::source` for `GraphError` (the fault of a failed node, a
  message error) and for `NodeFault` (the error a node returned, a refused
  update).
- Graph signature: `GraphBuilder::input(key)` and `GraphBuilder::output(key)`
  declare which state keys a graph takes and gives back; `Graph::signature()`
  lists them with their kinds (`Signature`). A key may be both. An undeclared
  key is refused at build (`UnknownKey`), a key declared twice too
  (`DuplicateInput`, `DuplicateOutput`). A graph without declarations has no
  inputs and no outputs.
- Defaults: `Kind::neutral()` (empty string, 0, 0.0, false, empty list, empty
  conversation) and `SchemaBuilder::state_with_default(key, kind, value)` for an
  explicit start value, checked at build (`KindMismatch`, `UnknownKey`).
  `Schema::default_value(key)` gives the start value of a key.
- `Graph::start_state(inputs)`: the start state of a run, from the given
  inputs plus the defaults of every other key. Refuses a declared input that is
  missing (`MissingInput`), a key that is not a declared input (`NotAnInput`),
  an unknown key, a value of the wrong kind, and an input given twice
  (`InputGivenTwice`). `State::new` is unchanged.
- Occurrences (`origin`): `Segment { node, index }`, `OccurrenceKey` (the path
  of a node occurrence through nested runs, empty at top level, written and
  serialized as `a/b[3]/c`), `RunId` (an opaque id supplied by the host) and
  `Origin` (run id and occurrence).
- `Context` knows where it runs: `with_run_id`, `run_id()`, `occurrence()`,
  `origin()`, `for_node(&NodeId)` and `child(Segment)`. `Context::new` gives an
  empty occurrence and the default run id. The run loop gives each node a
  context for its own occurrence.
- `SubGraph`: a node that runs another graph to its end. `SubGraph::call(graph)`
  then `.input(child_key, Input)` (`Input::From(parent_key)`,
  `Input::Config(parent_config_key)`, `Input::Const(value)`),
  `.config(child_config_key, Input)`, `.output(child_key, Output)`
  (`Output::Set(parent_key)`, `Output::Append(parent_list)`, renaming
  allowed), `.output_end_label(Output)` and `.on_failure(OnFailure)`;
  registered with `GraphBuilder::subgraph(id, call)`. The child starts from
  `Graph::start_state` with the mapped inputs and runs with a context for the
  calling node's occurrence; its state stays private and nothing is kept
  between two calls. `OnFailure::Propagate` (the default) fails the node with
  `SubGraphFailed`, whose `source()` is the child's `GraphError`. A nested run
  that stops before its end gives `SubGraphSuspended`, which
  `OnFailure::Capture` does not capture; nobody holds the child's sender, so
  this is a safety net.
- `OnFailure::Capture(Vec<CaptureUpdate>)`: a call whose child fails returns
  declared updates instead of its outputs, and the run goes on. Each
  `CaptureUpdate` is `Set` or `Append` into a parent key, from a
  `CaptureSource`: `Const(value)`, `From(parent_key)` (in a map body, the item
  key included) or `Reason` (the child's error message). Checked at build like
  outputs (`CaptureMismatch`); two capture updates that `Set` the same caller
  key are refused (`CaptureSetTwice`). Inside a map the captured appends are
  forwarded in item order and the item is recorded as finished.
- Pending writes and resume: `PendingWrites` (occurrence key to
  `PendingEntry { updates, witness }`, with `insert`, `get`, `merge`) and
  `Checkpoint::pending`. A map item that finishes records its appends under its
  occurrence with the item as witness (`Context::record_item`) while the map's
  superstep is still open; `Context::recorded_item(item)` gives what an earlier
  attempt recorded only when it ran on the same item value, and the map uses
  it instead of running the item again. A record made on another value is
  ignored and replaced.
  Every node of a superstep that returns updates records them the same way
  under its own occurrence (`Context::record`), and a node recorded by an
  earlier attempt is not run again (`Context::recorded`), a map that finished
  as a whole included. Entries of a node are dropped once its superstep is
  applied. The checkpoint of a failure, a cancel or a pause carries the run's
  pending writes; `Session::resume` and `Context::with_pending(checkpoint.pending)`
  (for `run`) hand them back, so a resumed run skips the finished nodes and
  items. Pending writes belong to the run being resumed: its nodes, the items
  of its maps and of a map directly inside one of them (`m[0]/m[1]`).
- A called graph restarts whole. When a call starts, the pending entries below
  its occurrence are removed, and inside it nothing is recorded or reused
  (`record` and `record_item` do nothing, `recorded` and `recorded_item` find
  nothing), however deep: the child rebuilds its own state, so an entry made
  inside it may come from a value or a loop round the restart does not
  reproduce. The call's own result is recorded by the run or the map that
  holds it, so a map whose items call a graph still skips its finished items
  on resume.
- `Observer::recorded(origin, entry)` (defaulted): fires when a map item is
  recorded, and when a node is recorded that shares its superstep with other
  nodes, so a host can persist pending entries, witness included, as they come
  and insert them later under `origin.occurrence`. A node alone in its
  superstep is recorded in memory without the event (a cancel right after it
  finished still keeps its result).
- `Checkpoint::with_pending`; `Context::pending()` copies what a recorder
  holds.
- `Map::on_item_failure` (`ItemFailure`), what a map does when the body of an
  item returns an error:
  - `Finish` (the default): every item runs to its end and the finished ones
    are recorded, then the map fails with the first error in item order;
  - `FailFast`: no item starts after the first error; items already running
    finish and are recorded, then the map fails with the first error in item
    order among the items that ended; items that never started are not
    recorded. Without `max_concurrency` every item starts at once, so it
    behaves like `Finish`;
  - `Capture(Vec<CaptureUpdate>)`: the item yields these appends instead,
    forwarded in item order and recorded like a result, so the map does not
    fail because of an item error. `From(key)` reads the item's own state,
    `Reason` is the error's message. A panic is not captured. Checked at build
    like a call's captures; a `Set` is refused (`MapBodySet`).
- `ReactAgent`: a ReAct agent as a graph with a declared signature, built with
  `ReactAgent { author, model, system, output, tools, tool_nodes,
  tool_concurrency, round_limit, thinking, enabled, inputs }.graph()` (an
  `Arc<Graph>`, ready for `SubGraph` or a `Map` body). It is the ReAct loop
  on the key `history`, the same code as the generator's generations; empty
  `tool_nodes` means one node, `tools`, runs every tool. Input `history`
  (conversation): a new call is a history holding one user input, a resumed call
  the history the host saved, and the loop always enters through the model call.
  `output` (`OutputMode`) is sent on every model call. Outputs `history`,
  `answer` (a string built like the generator-critic's `answer`, from the
  final step of the agent's last turn: the JSON text of its structured block
  when it has one, else its text blocks joined verbatim; empty when there is
  no final step) and, when the round limit's `OnLimit` is `Continue` or
  `End`, its `flag`. With `End`, the final step may be the limit's closing
  step. The configuration keys of
  `system` (`Source::Config`, strings), of `thinking` and of the limits are
  declared from them; each `Source::State` key of `system` is a string input.
  `enabled` (`Option<Key>`), when set, is an input holding a list of strings,
  wired to `LlmNode::enabled`: only the tools it names are offered to the model,
  and a call naming another tool is refused by its tool node. `inputs`
  (`Vec<(Key, Kind)>`): extra state keys declared as inputs of the graph, so the
  caller maps per-run values into the child state and the tools read them
  through the `&State` that `Tool::call` receives. As in `GeneratorCritic`, a
  key read by several sources of `system` is declared once, and a source,
  `enabled` or `inputs` key the graph already has is refused at build
  (`KeyCollision`). The run ends with
  `done`. A flag that names `history` or `answer` is refused
  (`FlagKeyMismatch`); a source, `enabled` or `inputs` key that names the
  flag is a key the graph already has (`KeyCollision`).
- Example `react_agent`: the agent graph called through `SubGraph`, its
  thinking read from the caller's configuration.
- `GeneratorCritic`: a generator and a critic writing in one shared history, as
  a graph with a declared signature. Built with
  `GeneratorCritic::new(GeneratorSeat, CriticSeat, max_critiques: Count)`
  (`final_generation` defaults to true, `inputs` to none,
  `empty_answer_critique` and `prepare` to `None`) then `.graph()`.
  `GeneratorSeat { author, model, system, output, thinking, tools, tool_nodes,
  tool_concurrency, round_limit, enabled }`; `CriticSeat { author, model,
  system, thinking, review_request, reasoning_field, thinking_description,
  is_valid_description, message_description, verdict }`, built with
  `CriticSeat::new(author, model, system)` (`reasoning_field` true, the rest
  `None`). Two seats with one author are refused (`SameAuthor`).
  - Texts: every text is a `Source`: the seats' `system`, the critic's
    `review_request` (default `Please evaluate the generator's response
    above.`), `empty_answer_critique` (default `The answer was empty. Write a
    complete answer.`) and the verdict descriptions. Each `State` key a source
    reads is a string input of the graph, each `Config` key a string
    configuration key; a key read by several sources is declared once, a key
    the graph already has is refused (`KeyCollision`).
  - What each seat reads. The generator: its system text, then
    `generator_history` from its own `Perspective`: the opening message, its
    earlier generations with their tool traffic and thinking, the critiques as
    framed user messages. The critic: its system text, then `conversation`
    before the answer it judges, rendered as the generator sees it (earlier
    answers as assistant messages, critiques as framed user messages) with
    every thinking block removed (an answer left with nothing is dropped and
    the user messages around it merged, as `render` merges them), then one
    user message `<generator_response>\n{answer}\n</generator_response>\n\n{review_request}`
    (the request and its blank line left out when it resolves to a blank
    text). `{answer}` is the JSON text of the answer's structured block when it
    has one, else its text blocks joined verbatim. That message stays a
    separate wire message even after a user message (the opening message in
    the first round).
  - Inputs: `conversation` (the shared history the critic reads),
    `generator_history` (the generator's working history, started from
    `conversation` when empty), the state keys of the text sources, the
    generator's `enabled` key when set (a list of strings, as for
    `ReactAgent`) and each key of `inputs`, for the generator's tools to read
    (as for `ReactAgent`). An `enabled` or `inputs` key the graph already has
    is refused (`KeyCollision`), and so is a generator round-limit flag that
    names `is_validated`.
  - Outputs, plain data: `answer` (string, the generator's last answer since
    the last user input, as in the critic's wrapped message; empty if none),
    `is_validated` (bool), `nb_generations` (int, the generator answers in
    `conversation` since its last user input), and the generator's
    round-limit flag when it has one, true when the round limit was reached
    in at least one generation of this call. The histories are not outputs: a
    host that resumes a call gives back the histories it saved from the
    observer. The run ends with `validated` or `exhausted`.
  - `generator_history` holds the generator's whole traffic and the critiques;
    `conversation` holds the user input, the answers (final step only) and the
    critiques. A critique is a turn of the critic holding one text step, the
    verdict's `message`, appended to both; nothing is appended on validation
    and nothing of the critic's reasoning enters a history.
  - The verdict. The default schema is always `thinking`, `is_valid`,
    `message`, in that order and all required, whatever the critic's
    `thinking` switch; `reasoning_field: false` removes `thinking`. Each field
    carries a description: `Explanation of the thought process and validation
    steps`, `Boolean indicating if the generated message is valid`, `Feedback
    message for the generator, empty if validation passed`, each replaced by
    its `*_description` source when set. `verdict: Some(VerdictOverride {
    schema, reader })` sends the caller's schema instead, and
    `reasoning_field` and the descriptions are ignored. `reader`
    (`VerdictReader`, a pure `Fn(&serde_json::Value) -> Result<Verdict,
    GraphError>`) turns the structured block into a `Verdict { is_valid,
    message }` (public); without one, the block is read as the default
    verdict and the build checks the schema: an object (`"type": "object"`)
    whose properties include `is_valid` (boolean) and `message` (string), both
    required, else `VerdictSchemaMismatch`. A missing structured block, a
    block that does not read as the default verdict (without a `reader`) or a
    rejection with a blank `message` (empty or whitespace only) fails the run
    with `Structured`; an error of the `reader` reaches the caller as the
    reader returned it.
  - `prepare` (`Option<Box<dyn Node>>`): when set, the graph's entry, run once
    before the routing. It may do I/O (it holds its capabilities as fields)
    and writes through ordinary updates, typically the opening message into
    `conversation`. It is checked through `Node::check` like any node; its
    failure fails the generator-critic (a caller captures it with
    `OnFailure::Capture`). Its writes are ordinary state of the call, so a
    resumed parent restarts the call, `prepare` included.
  - The entry is routed on how `conversation` ends: a generator answer goes to
    the critic and a critique (a turn of the critic) is routed as after a
    rejection (both are resumed calls); anything else, another author's turn
    or a turn without author included, goes to the generator. After an
    answer, more than `max_critiques` answers end `exhausted` without the
    critic. After a rejection, the generator answers again while fewer than
    `max_critiques` answers were given; at exactly `max_critiques` it answers
    once more, unassessed, when `final_generation`, else the run ends
    `exhausted` on the rejected answer. More answers than that (a resumed call
    whose budget, read from the configuration, is now smaller) end
    `exhausted`; the same rule applies to a critique found on entry. A `max_critiques` of 0 gives one generation and no critic
    call: the run ends `exhausted` (`is_validated` false, `nb_generations` 1),
    whatever `final_generation`.
  - An empty answer (its final step holds no non-blank text and no structured
    block) is rejected without calling the critic: where the critic would
    judge it, a critique of the critic holding the `empty_answer_critique`
    text (the default text when it resolves to a blank text) is appended to
    both histories and the run goes on as after a rejection. The empty answer counts as a generation and the budget rules
    are unchanged: an empty answer that is not judged (the unassessed final
    generation) ends `exhausted` with nothing appended.
  - With tools, the generation is the ReAct loop of `ReactAgent` inline in the graph;
    each generation opens a new generator turn, so the round limit counts per
    generation. A generation cut by `OnLimit::End` (its final step is the
    limit's closing step) is an empty answer: rejected without the critic
    where the critic would judge it, left out of what the critic reads in
    later rounds (the user messages around it merged, as for an answer left
    with nothing), and, as the last generation, an empty `answer`.
- Example `generator_critic`: the two seats called through `SubGraph`; a
  `prepare` node opens the conversation from a question id, the generator's
  system text is computed from the state, the critic's review request and
  thinking are read from the caller's configuration, and the caller reads the
  answer's text, `is_validated` and `nb_generations`.
- Example `subgraph_map`: a map whose body calls a graph, with a resume.
- `Node::check(&self, &Schema, CheckSite)`, defaulted to accept: a node checks
  itself against the schema of the graph it runs in, at a site
  (`CheckSite::Graph` or `CheckSite::MapBody`, non-exhaustive).
  `GraphBuilder::build` calls it on every node, however registered (`node`,
  `join`, `map`, `subgraph`), with `Graph`, and refuses the graph with
  `InvalidNode { node, source }`, `source` being the reason. `SubGraph` checks
  its mappings and, as a map body, refuses every output and capture that is
  not an `Append` (`MapBodySet`); `Map` checks its list and item keys, its
  limit and its captures, then its body with `MapBody`. A node that wraps
  another forwards `check` to it, site included.
- Reasons a call or a map is refused: `SubGraphInputUnmapped`,
  `SubGraphInputTwice`, `SubGraphNotAnInput`, `SubGraphSourceMismatch`,
  `SubGraphConfigUnmapped`, `SubGraphConfigTwice`, `SubGraphNotAConfig`,
  `SubGraphConfigMismatch`, `SubGraphNotAnOutput`, `SubGraphTargetMismatch`,
  `SubGraphSetTwice` (two outputs of one call, end label included, that `Set`
  the same caller key), `CaptureMismatch`, `CaptureSetTwice` (two capture
  updates of one call that `Set` the same caller key) and `MapBodySet`, each
  naming the key.

### Changed

- Release tag: `br-llm-graph/v{version}` from this version on (pin
  `tag = "br-llm-graph/v0.3.0"`); 0.1.0 and 0.2.0 keep their `v{version}` tags.
- Repository is a Cargo workspace: the crate lives in `crates/br-llm-graph/`.
  The package name and dependencies are unchanged.
- CI/CD: the libraries registry gate on pull requests, the release-line law
  (`main` and `release/<crate>-<major>.<minor>` are the only publishing
  branches), and a release chain that checks the registry target, computes the
  API listing, the API diff against the registry baseline and the
  dependencies, tags `br-llm-graph/v{version}`, creates the release and
  records it in the registry. Offline tests cover the wiring.
- `NodeFault::Returned` carries the error value the node returned
  (`NodeError`) instead of its text, so a host can downcast it to its own error
  type.
- `Map`, `ToolNode`, `LlmNode` and `Request` gain the fields above; struct
  literals must set them.
- `serde_json` is built with `preserve_order`: JSON objects keep the order
  of their keys, so a schema lists its properties in the order written.
- Examples `react_agent` and `generator_critic` show the graph forms above
  instead of hand-built loops, and so do `external_message`, `skills`,
  `background_task` and `react_goal_loop`: each builds its agent with
  `ReactAgent`, run on its own or called with `SubGraph`. With `ReactAgent`
  a `Source::State` key is a string, so `skills` holds the loaded skill's
  prompt in a string instead of a list.
- Depends on `br-llm-messages` 0.2.0: nothing in a `<message>` frame is
  escaped any more; the body and the `author` and `kind` attributes are
  rendered verbatim.
- Every `Observer` method receives an `&Origin` first. Events of the run loop
  carry the run's occurrence (empty at top level), events a node emits carry
  the node's occurrence, so events of a nested run are told from the
  parent's.
- `Context` gains private fields; build it with `Context::new`.
- `run` gives each run its own pending-writes recorder, seeded with what the
  given context holds, so two runs sharing one context never see each
  other's records.
- `Checkpoint` gains the field `pending`, read as empty from checkpoints
  written before it and not written when empty; build one with
  `Checkpoint::new`.
- `Map` is `{ list, item, body, max_concurrency, on_item_failure }`: the
  `output` and `results` fields are gone. The body runs on the state with `item` set, in a
  context for its own occurrence (`m[i]`); its updates are not applied to that
  state. Each must be an `Update::Append` to a list of the graph and the map
  forwards them, in item order whatever the completion order, so one body may
  append to several lists and they stay aligned. Any other update fails the
  map (`MapBodyNotAppend`), never silently dropped: the run-time safety net
  behind the build-time `MapBodySet`. A map run outside a graph, whose context
  names no node, fails with `MapWithoutOccurrence` when it has an item to run.
- A superstep is applied as a whole. When one of its nodes fails or panics,
  its updates are refused, or its edges or the end label cannot be resolved,
  nothing of it reaches the state: the failure checkpoint holds the state from
  before the superstep (plus the inputs received meanwhile), the cursor of the
  whole superstep and the pending entries of the nodes that finished. Before,
  the updates of the nodes that succeeded were applied, and a resume applied
  them a second time. A node whose updates are refused is not kept pending, so
  a resume runs it again. Observers see `applied` only for a superstep that is
  applied.
- `Map` runs its bodies in a rolling window: at most `max_concurrency` at
  once, a new one starting as soon as any running one finishes, never held by
  a slow earlier item.
- `GraphError::MapKeyMismatch` is `{ list, item }` and covers the list and
  item keys only; like every build-time reason of a node, it reaches the
  caller inside `InvalidNode`. A `Map` registered with `node` or `join` is now
  checked too.
- `run`, `Session::new` and `Session::resume` refuse a state whose schema is
  not the graph's (`StateSchemaMismatch`) and a configuration whose keys are
  not the graph's configuration keys (`ConfigSchemaMismatch`), before
  anything runs; `run` returns the failure with the given state.
  `Session::new` returns `Result<Session, GraphError>`. Nested runs (a call,
  a map) build their state and configuration from the graph they run, so
  they are unaffected.
- A key the schema does not declare gives `UnknownKey` wherever it is read
  or updated, in a state and in a configuration alike: typed reads, the
  values a node or a source reads, `Append` and the conversation updates.
  Before, a read gave `MissingKey` and an update `AppendNotList` or
  `NotConversation`. `MissingKey` stays for a declared key with no value,
  `AppendNotList` and `NotConversation` for a declared key of the wrong kind.
- A run given an empty cursor fails with `GraphError::EmptyCursor` before
  running anything, instead of `AmbiguousEnd` naming no label.
- `Schema` gains the public field `defaults` (serialized only when not empty,
  so earlier schemas and checkpoints still load) and no longer implements
  `Eq`, since a default is a `Value`.

### Removed

- `ReactLoop` is no longer public: it is the internal building block of
  `ReactAgent` and of the generator's generations. Use `ReactAgent`, run on
  its own or called with `SubGraph`; its rules (tool partition, routing after
  the model call, tool concurrency, enabled key, round limit) are unchanged.

### Fixed

- A node that panics while its `run` call starts, before it returns its
  future, is caught like a panic inside the future: the node fails with
  `NodeFault::Panic` instead of unwinding through the run.
- `State::list` and `Config::list` on a key that holds another kind give
  `KindMismatch` with a list as the expected kind (`list<int>` for an int
  key: a list read names no element kind), not the key's own kind.

## 0.2.0 - 2026-09-22

### Removed

- `Message`, the inbox item type, is no longer public. It is an internal
  detail of the run loop: a runner drives a session through
  `Sender::send`/`pause`/`resume`/`cancel` and hands the `Inbox` to `run` or
  `Session`, never constructing or matching an inbox item itself.

## 0.1.0 - 2026-09-22

### Added

- Repository scaffold: crate manifest, governance files (LICENSE,
  CONTRIBUTING, SECURITY, SUPPORT, PR template, issue-template config),
  `.gitignore`, and `deny.toml`.
- CI (`ci.yml`): fmt + clippy + test, MSRV 1.89 build, `cargo-deny`,
  `cargo-machete`, `cargo semver-checks`, changelog + README-pin check,
  shellcheck, and trufflehog secret scan.
- CD (`release-tags.yml`): auto-tag and release the crate version on merge to
  `main`, inert while the version is the `0.0.0` scaffold.
- Value objects (`value`): validated `Key`, `NodeId`, `EndLabel` newtypes
  (`[a-z][a-z0-9_]*`), serde through `try_from` so illegal JSON is refused.
- Typed state (`state`): `Value` (with a `Finite` float that refuses NaN and
  infinities), `Kind`, `Schema` + `SchemaBuilder`, `State` and `Config` with
  present/undeclared/kind checks at construction and re-validated at load,
  typed getters, `State::derive`, canonical JSON at `SCHEMA_VERSION`
  `br-llm-graph/1`.
- Updates (`update`): `Set`, `Append`, `Input`, `PushTurn`, `PushStep`,
  `PushResult`; atomic `State::apply_batch` with the `SetConflict` rule.
- Graph (`graph`): `Node`/`Edge` traits, `NodeFuture`, `Target`, `Context`,
  `IdSource`, the `FnNode`/`FnEdge`/`Always` adapters, the `Map` fan-out node,
  and `GraphBuilder`/`Graph` with build-time refusals (duplicate id, unknown
  entry, missing edge, edge from unknown node, duplicate edge, map key
  mismatch).
- Run loop (`run`): the Pregel superstep engine with static fan-out, the single
  join rule, per-node application in declaration order, caught node panics, an
  edge to a node the graph does not contain refused with `UnknownNode` rather
  than a silently dropped branch, the runtime-neutral inbox (`Sender`/`Inbox`),
  `Pause`/`Resume`/`Cancel` commands, `Cursor`, `Checkpoint`, `Outcome`, and
  `RunFailure`.
- Session (`session`): `Session` with `new`/`resume`/`sender`/`serve`/
  `run_once`, queueing inputs during a run and relaunching after an end.
- Observer (`observe`): the `Observer` trait with empty defaults and
  `NoopObserver`.
- React module (`react`): the `Model` and `Tool` traits, `Request`,
  `OutputMode`, `ToolSpec`, `StreamSink`, `ToolOutput`; the `wire`, `complete`,
  `structured`, `pending_calls` (`state`/`key`/`author` in, borrowed calls out),
  `pending_unsafe_calls` helpers; `LlmNode` (with `Source`), `ToolNode`, and
  `ReactLoop` with a strict tool-set partition check (a tool node tool the LLM
  node does not declare is rejected) and a fail-closed loop edge (a pending call
  no tool node runs is refused, never left to livelock).
- Errors (`error`): a single `GraphError` type with a hand-written `Display`
  and `NodeFault` for node failures, including `ToolNotDeclared` and
  `PendingToolUnsatisfiable`.
- Run loop: a `Cancel` that arrives while a fast superstep is completing returns
  the still-unmutated state and reruns the superstep on resume, so no node
  update is applied twice; an inbox input already drained during that superstep
  is folded into the `Cancelled`, `Paused` and node-failure checkpoints, so an
  input the lib has taken ownership of survives `Session::resume`.
- Examples: `react_agent`, `react_goal_loop`, `generator_critic`,
  `background_task`, `external_message`, `skills`, `rehydration`, over a shared
  scripted-model and fake-tool harness in `examples/common`.

