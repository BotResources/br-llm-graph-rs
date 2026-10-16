use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_messages::Author;

use crate::error::GraphError;
use crate::graph::{Always, Graph, GraphBuilder, Limit, Target};
use crate::react::agent_answer::AnswerNode;
use crate::react::agent_build::{AgentKeys, declare_limit, round_limit_flag, tools_node_clash};
use crate::react::agent_call::{AgentCall, ThreadBinding};
use crate::react::declarations::Declarations;
use crate::react::enter::{EnterNode, enter_edge};
use crate::react::given::{History, Model, Prompt};
use crate::react::llm_node::LlmNode;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::RoundLimit;
use crate::react::thread_store::ThreadStore;
use crate::react::tool::Tool;
use crate::react::turn_clients::TurnClients;
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
///   JSON text of its structured block when it has one, else its text blocks
///   joined verbatim; empty when there is no final step); when the round
///   limit's `OnLimit` is `Continue` or `End`, its `flag` (bool, true when the
///   limit was reached), with `End` the final step may be the limit's closing
///   step; durable, `thread` (string), the id of the agent's thread.
///   `conversation` is private: neither an input nor an output, never part
///   of a checkpoint of the agent's graph, so a run of that graph resumed
///   from a checkpoint enters again through `enter`.
/// - Configuration: each configuration key the functions declare, of its
///   kind, the tool concurrency and the round limit when they read a
///   configuration key.
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
/// JSON text in `answer`. The settings of a client, its level of reasoning
/// included, are the host's: an agent that needs another model or another
/// level is given another client.
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
    pub author: Author,
    /// The client of each turn, asked at its first round.
    pub model: Model<ReactAgent>,
    /// The system text; one that resolves blank gives none.
    pub system: Prompt<ReactAgent>,
    /// The messages sent. `None`: the agent's conversation rendered for
    /// `author`.
    pub history: Option<History<ReactAgent>>,
    pub output_mode: OutputMode,
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all; that id is the graph's own even with no tool, so a
    /// round-limit node named `tools` collides with it (`DuplicateNode`).
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    /// At most that many tool calls run at once, across all the tool nodes
    /// together. `None`: every call at once.
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
    /// A state key holding the names of the tools offered on this run.
    pub enabled: Option<Key>,
    /// Extra state keys the caller maps per run, read by the tools.
    pub inputs: Vec<(Key, Kind)>,
    /// The store of the agent's threads. `None`: the agent works in memory.
    pub thread_store: Option<Arc<dyn ThreadStore>>,
}

impl ReactAgent {
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
            .chain(self.history.as_ref().map(History::reads));
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
        let mut inputs = vec![keys.opening.clone()];
        inputs.extend(keys.thread.clone());
        inputs.extend(declared);

        let call = AgentCall {
            opening: keys.opening.clone(),
            position: keys.position.clone(),
            thread: match (&keys.thread, self.thread_store) {
                (Some(key), Some(store)) => Some(ThreadBinding {
                    store,
                    key: key.clone(),
                }),
                _ => None,
            },
            turns: None,
        };
        let clients = Arc::new(TurnClients::default());
        let tools_node = NodeId::new("tools")?;
        let clash = tools_node_clash(&self.tool_nodes, self.round_limit.as_ref(), &tools_node);
        let tool_nodes = if self.tool_nodes.is_empty() && !self.tools.is_empty() {
            vec![(tools_node, self.tools.clone())]
        } else {
            self.tool_nodes
        };
        let answer_call = call.clone();
        let answer_flag = flag.clone().zip(self.round_limit.clone());
        let react = ReactLoop {
            llm: llm.clone(),
            tool_nodes,
            after: Target::Node(answer_node.clone()),
            tool_concurrency: self.tool_concurrency,
            round_limit: self.round_limit,
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
            call: Some(call.clone()),
            seat: None,
            clients: clients.clone(),
        };

        let mut builder = keys
            .declare_private(GraphBuilder::new(schema))
            .entry(enter.clone())
            .node(
                enter.clone(),
                EnterNode {
                    conversation: keys.conversation.clone(),
                    call: call.clone(),
                    clients,
                },
            )
            .edge(enter, enter_edge(call, llm.clone(), answer_node.clone()));
        for key in inputs {
            builder = builder.input(key);
        }
        builder = builder.output(keys.answer.clone());
        if let Some(flag) = flag {
            builder = builder.output(flag);
        }
        if let Some(thread) = &keys.thread {
            builder = builder.output(thread.clone());
        }
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
