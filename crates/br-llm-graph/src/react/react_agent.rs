use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_messages::Author;

use crate::describe::shape::AgentKind;
use crate::error::GraphError;
use crate::graph::{Always, Count, Graph, GraphBuilder, Limit, Target};
use crate::react::agent_answer::AnswerNode;
use crate::react::agent_build::{
    AgentKeys, declare_count, declare_limit, round_limit_flag, split_tools, tools_node_clash,
};
use crate::react::bounds::Bounds;
use crate::react::declarations::Declarations;
use crate::react::enter::{EnterNode, enter_edge};
use crate::react::given::{History, Model, Prompt};
use crate::react::llm_node::LlmNode;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::RoundLimit;
use crate::react::thread_store::ThreadStore;
use crate::react::tool::Tool;
use crate::state::{Kind, Schema};
use crate::value::{EndLabel, Key, NodeId};

/// A ReAct agent as a graph, to call with `SubGraph` or run on its own.
///
/// The graph enters through `enter`, then runs the ReAct loop on its private
/// key `conversation`: the model call, then, while the reply calls tools, the
/// tool nodes that run the calls and the model call again; when a reply
/// calls no tool, a node reads the answer and the graph ends with `done`.
///
/// Built with a `thread_store`, the agent is durable: it works in a thread
/// and resumes from it (see `ThreadStore`). `enter` rehydrates
/// `conversation` from the thread; a call whose turn the thread holds
/// completed goes to the answer without calling the model. Every step, tool
/// results item and turn end is written to the thread first, then applied to
/// `conversation`; the turn stays open across its rounds. Without one, the
/// agent works in memory: `conversation` starts as `opening` and lives for
/// the current call. Both run the same loop, read the same functions and
/// give the same outputs and errors; they differ in the turn a reply goes
/// on in. In memory, the rule of the ReAct loop is literal: a reply is added
/// as a new step of the author's last turn when that turn awaits a step, so
/// an own turn the opening ends with, awaiting a step, is continued. Durable,
/// a reply goes on in the turn of the call's rank only: a turn of the thread
/// that is not the call's, or one repaired on entry, is never continued, and
/// the call opens its turn of the next rank.
///
/// The model call is built from what the runner gives it (see `Given`), each
/// subject a value or a function of the agent's context, which holds the
/// state, the configuration, the agent's conversation and the round (the
/// rounds already answered in the agent's turn): `model` gives the client of
/// the turn, asked once at the turn's first round and kept for its other
/// rounds, so a turn is run by one client (a turn continued in memory is
/// asked at its first round of this call, and its function then reads the
/// rounds the turn had already answered); a schedule (`Given::schedule`)
/// advances per turn of the call, so a turn opened after a repaired one gets
/// the next client. `system` gives the system text, asked at each round
/// (one that resolves blank gives none), `history` the messages, also asked
/// at each round, by default the conversation rendered
/// for `author`. A function shapes the request only: the reply is added to
/// the conversation whatever the history sent. A change of client never
/// changes the author.
///
/// - Inputs: `opening` (conversation), what the caller adds before the agent
///   answers; durable, user inputs ending with the trigger, the user input
///   the call answers (`OpeningNotInputs` otherwise), each stored in the
///   thread unless it holds its id. Durable, `thread` (string): the id of a
///   thread the store holds (`ThreadNotFound` otherwise; any text that is not
///   a UUIDv7 is refused, `ThreadId`), or the empty string to have the store
///   create one under an id from the id source. Each state key the functions
///   declare is an input of its kind, and so is the `enabled` key, a list of
///   strings, when set. Each key of `inputs` is an input of its kind, for the
///   tools to read through the state they receive.
/// - Outputs: `answer` (string, the final step of the agent's last turn: the
///   JSON text of its structured block when it has one, which fits the shape
///   when the output mode is structured, else its text blocks joined
///   verbatim; empty when there is no final step); when the round
///   limit's `OnLimit` is `Continue` or `End`, its `flag` (bool, true when the
///   limit was reached), with `End` the final step may be the limit's closing
///   step; durable, `thread` (string), the id of the agent's thread.
///   `conversation` is private: neither an input nor an output, never part
///   of a checkpoint of the agent's graph, so a run of that graph resumed
///   from a checkpoint enters again through `enter`.
/// - Configuration: each configuration key the functions declare, of its
///   kind, the tool concurrency, the round limit and the reply retries when
///   they read a configuration key. The edge of `enter` resolves the reply
///   retries, then both limits, before anything is routed: a refused one
///   (`CountNegative`, `LimitNotPositive`) is the run's error itself, and
///   the model is not called.
///
/// With `enabled`, only the tools the key names are offered to the model, and
/// a call naming another tool is answered with an error result, never
/// executed. One key given twice with one kind, and so one meaning, is
/// declared once: a key read by several functions, a key a function reads
/// that is also `enabled` or a key of `inputs`, a configuration key a
/// function reads that is also the key of a limit. A key given with another
/// kind than the graph already has is refused (`KeyCollision`), and so is a
/// key the graph keeps for itself, whatever its kind: `opening`, `answer`,
/// `conversation`, `turn_rank` (the private key where the call stands among
/// its turns), `thread` when durable and the round limit's flag. A flag that
/// names `answer`, `opening`, `thread` or `conversation` is refused
/// (`FlagKeyMismatch`), before the rules of the ReAct loop below.
///
/// `output_mode` is sent on every model call; with `OutputMode::Structured`, the
/// final step carries the structured block and reaches the caller as the
/// JSON text in `answer`. The final reply of a turn, the one that calls no
/// tool, is then checked against its shape (see `LlmCall`); a reply that
/// calls tools is not, and neither is the closing step of the round limit
/// `End`, which no model wrote. The settings of a client, its level of
/// reasoning included, are the host's: an agent that needs another model or
/// another level is given another client.
///
/// A reply that cannot be used (refused by the client as unusable or
/// malformed, or a final reply that does not fit) is asked for again while
/// `reply_retries` are left, as an `LlmCall` asks: the model is asked again
/// for the next attempt, and the client it gives is kept for the turn's
/// other rounds. A reply asked for again never becomes part of the
/// conversation and counts against no round limit, though the observer
/// hears its stream events (see `Observer::stream`); when no retry is left,
/// the run fails (`Model` or `Structured`). Durable, with a structured
/// output mode, the events of a step reach the thread once it is checked,
/// so a reply asked for again is never stored (the agent does not stream
/// into its thread live); in the plain mode, a reply the client refuses
/// after some of its events reached the thread fails the open turn, and is
/// asked for again in the turn of the next rank (see `ThreadStore`).
///
/// Two of `tools` with one name are refused first, as a request no client
/// accepts (`InvalidNode` naming `llm`, its reason `Request` carrying
/// `DuplicateToolName`), the same for a generator seat (naming `generate`).
/// Then each tool of `tool_nodes` must be one of `tools` (`ToolNotDeclared`),
/// and each of `tools` run by exactly one tool node (`ToolNotCovered`,
/// `ToolCoveredTwice`). A pending call no tool node runs fails the run
/// (`PendingToolUnsatisfiable`). The graph's own nodes are `enter`, `llm`,
/// `answer` and, without a split, `tools`: a tool node or a round-limit node
/// of one of these ids collides with it (`DuplicateNode`).
///
/// The round limit counts the rounds of the agent's turn (see `RoundLimit`).
pub struct ReactAgent {
    /// Who answers: the author of the agent's turns.
    pub author: Author,
    /// The client of each turn, asked at its first round.
    pub model: Model<ReactAgent>,
    /// The system text; one that resolves blank gives none.
    pub system: Prompt<ReactAgent>,
    /// The messages sent. `None`: the agent's conversation rendered for
    /// `author`.
    pub history: Option<History<ReactAgent>>,
    /// The output mode sent on every model call.
    pub output_mode: OutputMode,
    /// The agent's tools, fixed when the graph is built.
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all; that id is the graph's own even with no tool, so a
    /// round-limit node named `tools` collides with it (`DuplicateNode`).
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    /// At most that many tool calls run at once, across all the tool nodes
    /// together. `None`: every call at once.
    pub tool_concurrency: Option<Limit>,
    /// The bound on the tool rounds of one turn. `None`: no bound.
    pub round_limit: Option<RoundLimit>,
    /// A state key holding the names of the tools offered on this run.
    pub enabled: Option<Key>,
    /// The times a reply that cannot be used is asked for again. `None`: 0,
    /// the run fails on the first.
    pub reply_retries: Option<Count>,
    /// The text sent after the request when a reply is asked for again, each
    /// `{reason}` in it replaced by why the reply could not be used; one
    /// that resolves blank sends the request alone. `None`: `Your reply
    /// could not be used: {reason}. Reply again.`
    pub reminder: Option<Prompt<ReactAgent>>,
    /// Extra state keys the caller maps per run, read by the tools.
    pub inputs: Vec<(Key, Kind)>,
    /// The store of the agent's threads. `None`: the agent works in memory.
    pub thread_store: Option<Arc<dyn ThreadStore>>,
}

impl ReactAgent {
    /// The graph of this agent, or the first refusal of its build (see the
    /// type's documentation).
    pub fn graph(self) -> Result<Arc<Graph>, GraphError> {
        let keys = AgentKeys::new(self.thread_store.is_some())?;
        let llm = NodeId::new("llm")?;
        let enter = NodeId::new("enter")?;
        let answer_node = NodeId::new("answer")?;

        let mut schema = Schema::builder()
            .state_by_name(keys.opening.clone(), Kind::Conversation)
            .state_by_name(keys.answer.clone(), Kind::Str);
        if let Some(thread) = &keys.thread {
            schema = schema.state_by_name(thread.clone(), Kind::Str);
        }
        let reply_retries = self.reply_retries.unwrap_or(Count::Fixed(0));
        schema = declare_count(schema, &reply_retries);
        schema = declare_limit(schema, self.tool_concurrency.as_ref());
        let flag = self.round_limit.as_ref().and_then(round_limit_flag);
        if let Some(round_limit) = &self.round_limit {
            schema = declare_limit(schema, Some(&round_limit.max_rounds));
        }
        let mut schema = schema.build();
        // The private keys are declared while the other keys are, so a key
        // that takes their name collides with them.
        keys.reserve(&mut schema);
        if let Some(flag) = &flag {
            schema.state.entry(flag.clone()).or_insert(Kind::Bool);
        }
        let mut declarations = Declarations::new(schema, keys.reserved(flag.as_ref()));
        let reads = [self.model.reads(), self.system.reads()]
            .into_iter()
            .chain(self.history.as_ref().map(History::reads))
            .chain(self.reminder.as_ref().map(Prompt::reads));
        for reads in reads {
            declarations.reads(reads)?;
        }
        if let Some(key) = &self.enabled {
            declarations.input(key, Kind::list(Kind::Str))?;
        }
        for (key, kind) in self.inputs {
            declarations.input(&key, kind)?;
        }
        if let Some(flag) = &flag
            && keys.names_flag(flag)
        {
            return Err(GraphError::FlagKeyMismatch { key: flag.clone() });
        }
        let (mut schema, declared) = declarations.finish();
        keys.release(&mut schema);
        reply_retries.check(&schema)?;

        let call = keys.call(self.thread_store);
        let tools_node = NodeId::new("tools")?;
        let clash = tools_node_clash(&self.tool_nodes, self.round_limit.as_ref(), &tools_node);
        let tool_nodes = split_tools(self.tool_nodes, &self.tools, &tools_node);
        let bounds = Bounds::new(
            [&reply_retries],
            [
                self.tool_concurrency.as_ref(),
                self.round_limit.as_ref().map(|limit| &limit.max_rounds),
            ],
        );
        let answer_call = call.clone();
        let answer_flag = flag.clone().zip(self.round_limit.clone());
        let react = ReactLoop {
            llm: llm.clone(),
            tool_nodes,
            after: Target::Node(answer_node.clone()),
            thread: call.thread.clone(),
        };
        let llm_node = LlmNode {
            key: keys.conversation.clone(),
            author: self.author.clone(),
            model: self.model,
            system: self.system,
            history: self.history,
            tools: self.tools,
            enabled: self.enabled,
            output_mode: self.output_mode,
            tool_concurrency: self.tool_concurrency,
            round_limit: self.round_limit,
            reply_retries,
            reminder: self.reminder,
            call: call.clone(),
            seat: None,
        };

        let builder = keys
            .declare_private(GraphBuilder::new(schema))
            .agent(AgentKind::ReactAgent)
            .entry(enter.clone())
            .node(
                enter.clone(),
                EnterNode {
                    conversation: keys.conversation.clone(),
                    call: call.clone(),
                },
            )
            .edge(
                enter,
                enter_edge(call, bounds, llm.clone(), answer_node.clone()),
            );
        let builder = keys.signature(builder, declared, &[], flag.as_ref());
        let builder = react.add(builder, llm_node)?;
        if let Some(clash) = clash {
            return Err(clash);
        }
        let graph = builder
            .node(
                answer_node.clone(),
                AnswerNode {
                    conversation: keys.conversation,
                    answer: keys.answer,
                    author: self.author,
                    call: answer_call,
                    flag: answer_flag,
                },
            )
            .edge(answer_node, Always(Target::End(EndLabel::new("done")?)))
            .build()?;
        Ok(Arc::new(graph))
    }
}
