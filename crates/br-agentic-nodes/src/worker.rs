use br_agentic_contract::{InboxContext, InboxPort, Queue, Session, ThreadAccess, ToolSet};
use br_graph_contract::CancelView;
use br_llm_messages::{
    AITurn, Author, ClosureReason, PlainText, ProjectedThread, Turn, TurnClosure, TurnId,
};

use crate::fault::Fault;
use crate::ids::entry_id;
use crate::limits::{StepLimit, Steps, Timeouts};
use crate::sandbox::FileSystem;

pub struct Worker<'a> {
    pub(crate) access: &'a dyn ThreadAccess,
    pub(crate) inbox: Option<&'a dyn InboxPort>,
    pub(crate) context: InboxContext,
    pub(crate) session: Session<'a>,
    pub(crate) queue: Queue,
    pub(crate) author: Author,
    pub(crate) system: Option<PlainText>,
    pub(crate) tools: ToolSet,
    pub(crate) files: Option<FileSystem>,
    pub(crate) steps: Steps,
    pub(crate) timeouts: Timeouts,
    pub(crate) cancel: CancelView,
    pub(crate) stored_items: usize,
}

pub struct WorkerParts<'a> {
    pub access: &'a dyn ThreadAccess,
    pub inbox: Option<&'a dyn InboxPort>,
    pub context: InboxContext,
    pub author: Author,
    pub system: PlainText,
    pub tools: ToolSet,
    pub step_limit: StepLimit,
    pub timeouts: Timeouts,
    pub cancel: CancelView,
}

impl<'a> Worker<'a> {
    pub fn new(parts: WorkerParts<'a>) -> Self {
        Self {
            access: parts.access,
            inbox: parts.inbox,
            context: parts.context,
            session: Session::new(parts.access, parts.context.thread, parts.context.agent),
            queue: Queue::new(),
            author: parts.author,
            system: Some(parts.system),
            tools: parts.tools,
            files: None,
            steps: Steps::new(parts.step_limit),
            timeouts: parts.timeouts,
            cancel: parts.cancel,
            stored_items: 0,
        }
    }

    pub fn steps_remaining(&self) -> u32 {
        self.steps.remaining()
    }

    pub fn has_inbox(&self) -> bool {
        self.inbox.is_some()
    }
}

pub(crate) fn open_ai_turn(projected: &ProjectedThread) -> Option<&AITurn> {
    projected
        .turns()
        .iter()
        .rev()
        .find_map(Turn::as_ai)
        .filter(|turn| !turn.is_closed())
}

impl Worker<'_> {
    pub async fn load(&self) -> Result<ProjectedThread, Fault> {
        Ok(self
            .access
            .load(&self.context.thread, self.context.agent)
            .await?)
    }

    pub async fn close_turn(&self, turn: TurnId, reason: ClosureReason) -> Result<(), Fault> {
        let closure = TurnClosure::new(entry_id()?, reason);
        self.access
            .close_turn(&self.context.thread, self.context.agent, turn, closure)
            .await?;
        Ok(())
    }

    pub(crate) async fn interrupt_open_turn(&self) -> Result<(), Fault> {
        let projected = self.load().await?;
        match open_ai_turn(&projected) {
            Some(turn) => {
                self.close_turn(turn.id(), ClosureReason::interrupted())
                    .await
            }
            None => Ok(()),
        }
    }

    pub(crate) async fn close_open_quietly(&self, reason: ClosureReason) {
        let Ok(projected) = self.load().await else {
            return;
        };
        if let Some(turn) = open_ai_turn(&projected) {
            let _ = self.close_turn(turn.id(), reason).await;
        }
    }
}
