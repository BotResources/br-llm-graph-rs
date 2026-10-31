use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use br_agentic_contract::{
    AiTurnRef, InboxContext, InboxPort, Queue, Session, ThreadAccess, ToolSet,
};
use br_graph_contract::CancelView;
use br_llm_messages::{
    AgentId, Author, ClosureReason, Conversation, EntryId, PlainText, TurnClosure,
};

use crate::fault::Fault;
use crate::ids::entry_id;
use crate::limits::{StepLimit, Steps, Timeouts};
use crate::read::RunningRead;
use crate::sandbox::FileSystem;

pub(crate) type MailGate = Arc<Mutex<HashSet<EntryId>>>;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Counters {
    pub(crate) rounds: u32,
    pub(crate) refusals: u32,
}

impl Counters {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

pub struct Worker<'a> {
    pub(crate) session: Session<'a>,
    pub(crate) inbox: Option<Arc<dyn InboxPort>>,
    pub(crate) context: InboxContext,
    pub(crate) queue: Queue,
    pub(crate) read: Option<RunningRead>,
    pub(crate) author: Option<Author>,
    pub(crate) system: Option<PlainText>,
    pub(crate) tools: ToolSet,
    pub(crate) files: Option<FileSystem>,
    pub(crate) steps: Steps,
    pub(crate) timeouts: Timeouts,
    pub(crate) cancel: CancelView,
    pub(crate) counters: Counters,
    pub(crate) agents: Vec<AgentId>,
    pub(crate) known: Vec<(AgentId, Author)>,
    pub(crate) gate: MailGate,
}

pub struct WorkerParts<'a> {
    pub access: &'a dyn ThreadAccess,
    pub inbox: Option<Arc<dyn InboxPort>>,
    pub context: InboxContext,
    pub step_limit: StepLimit,
    pub timeouts: Timeouts,
    pub cancel: CancelView,
}

impl<'a> Worker<'a> {
    pub async fn open(parts: WorkerParts<'a>) -> Result<Self, Fault> {
        let session = Session::start(parts.access, parts.context.thread).await?;
        Ok(Self {
            session,
            inbox: parts.inbox,
            context: parts.context,
            queue: Queue::new(),
            read: None,
            author: None,
            system: None,
            tools: ToolSet::new(Vec::new(), Vec::new())?,
            files: None,
            steps: Steps::new(parts.step_limit),
            timeouts: parts.timeouts,
            cancel: parts.cancel,
            counters: Counters::default(),
            agents: vec![parts.context.agent],
            known: Vec::new(),
            gate: Arc::default(),
        })
    }

    #[must_use]
    pub fn with_author(mut self, author: Author) -> Self {
        self.known.retain(|(agent, _)| *agent != self.context.agent);
        self.known.push((self.context.agent, author.clone()));
        self.author = Some(author);
        self
    }

    #[must_use]
    pub fn with_system(mut self, system: PlainText) -> Self {
        self.system = Some(system);
        self
    }

    #[must_use]
    pub fn with_tools(mut self, tools: ToolSet) -> Self {
        self.tools = tools;
        self
    }

    pub fn steps_remaining(&self) -> u32 {
        self.steps.remaining()
    }

    pub fn steps_made(&self) -> u32 {
        self.steps.made()
    }

    pub fn has_inbox(&self) -> bool {
        self.inbox.is_some()
    }

    pub fn conversation(&self) -> &Conversation {
        self.session.conversation()
    }

    pub(crate) fn author(&self) -> Result<&Author, Fault> {
        self.author
            .as_ref()
            .ok_or_else(|| Fault::Value("the worker has no author name".to_owned()))
    }

    pub(crate) fn turn_ref(&self, turn: br_llm_messages::TurnId) -> Result<AiTurnRef, Fault> {
        Ok(AiTurnRef {
            id: turn,
            agent: self.context.agent,
            author: self.author()?.clone(),
        })
    }

    pub async fn close_turn(
        &mut self,
        turn: &AiTurnRef,
        reason: ClosureReason,
    ) -> Result<(), Fault> {
        let closure = TurnClosure::new(entry_id()?, reason);
        self.session.close_turn(turn, closure).await?;
        Ok(())
    }

    pub(crate) async fn close_open_turns(
        &mut self,
        reason: &ClosureReason,
        strict: bool,
    ) -> Result<(), Fault> {
        for agent in self.agents.clone() {
            while let Some(turn) = self.session.open_ai_turn(agent) {
                let closed = self.close_turn(&turn, reason.clone()).await;
                if let Err(fault) = closed {
                    if strict {
                        return Err(fault);
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}
