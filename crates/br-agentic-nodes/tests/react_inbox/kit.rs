use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use br_agentic_contract::memory::{FixedThreadPort, FixedToolsPort};
use br_agentic_contract::{
    Attachment, DocumentRef, InboxBatch, InboxEntry, InboxItem, InboxPort, MailId, NewMail,
    ThreadAccess,
};
use br_agentic_nodes::{
    AuthorSource, FallbackList, ModelClient, Ports, ReactAgent, ThinkingExpectation,
};
use br_llm_clients_core::{
    BoxFuture, CallOutcome, Client, ClientRequest, ClientSettings, StreamSink,
};
use br_llm_messages::{
    AgentId, Conversation, ConversationId, EntryId, EntryView, HumanMessage, IncomingEntry,
    PlainBlock, TurnEntry, TurnId,
};

use crate::common::*;

pub struct Yielding(pub Arc<Scripted>);

impl Client for Yielding {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            self.0.call(request, sink).await
        })
    }

    fn settings(&self) -> &ClientSettings {
        self.0.settings()
    }
}

pub fn lagging(scripted: &Arc<Scripted>) -> FallbackList {
    FallbackList::new(vec![ModelClient::new(
        Arc::new(Yielding(scripted.clone())),
        ThinkingExpectation::NotExpected,
    )])
    .unwrap()
}

pub fn build(
    rig: &Rig,
    inbox: Option<Arc<dyn InboxPort>>,
    model: &Arc<Scripted>,
    author_name: &str,
) -> ReactAgent<Doc> {
    let ports = match inbox {
        Some(inbox) => rig.ports().with_inbox(inbox),
        None => rig.ports(),
    };
    let mut spec = rig.react_spec(lagging(model));
    spec.author = AuthorSource::Fixed(author(author_name));
    ReactAgent::new("agent", ports, rig.settings.clone(), spec)
}

pub fn spied(rig: &Rig, spy: &Arc<crate::spy::Spy>, model: &Arc<Scripted>) -> ReactAgent<Doc> {
    build(rig, Some(spy.clone()), model, "Agent")
}

pub fn through_access(
    rig: &Rig,
    access: Arc<dyn ThreadAccess>,
    inbox: Arc<dyn InboxPort>,
    model: &Arc<Scripted>,
) -> ReactAgent<Doc> {
    let ports = Ports::new(
        Arc::new(FixedThreadPort::new(&rig.record, rig.thread)),
        rig.agents.clone(),
        access,
        rig.prompts.clone(),
        Arc::new(FixedToolsPort::new(&rig.record, rig.tools.clone())),
        rig.workspace.clone(),
    )
    .with_inbox(inbox);
    ReactAgent::new(
        "agent",
        ports,
        rig.settings.clone(),
        rig.react_spec(lagging(model)),
    )
}

pub fn on_nth_call(model: &Arc<Scripted>, nth: usize, act: impl Fn() + Send + Sync + 'static) {
    let seen = AtomicUsize::new(0);
    model.on_call(move |_| {
        if seen.fetch_add(1, Ordering::SeqCst) + 1 == nth {
            act();
        }
    });
}

pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

pub fn human(text: &str) -> (InboxEntry, EntryId) {
    let message = HumanMessage::new(entry_id(), human_id(), vec![text_block(text)]).unwrap();
    let id = message.id();
    (InboxEntry::new(IncomingEntry::from(message)).unwrap(), id)
}

pub fn item(entry: &InboxEntry) -> InboxItem {
    InboxItem::Entry(entry.clone())
}

pub fn batch(items: Vec<InboxItem>, next: Option<&str>) -> InboxBatch {
    InboxBatch {
        items,
        next: next.map(br_agentic_contract::Cursor::new),
    }
}

pub fn empty_batch() -> InboxBatch {
    batch(Vec::new(), None)
}

pub fn new_mail(to: &[AgentId], cc: &[AgentId], subject: &str, body: &str) -> NewMail {
    NewMail {
        id: mail_id(),
        author: author("Sender Name"),
        source: ConversationId::from_uuid(uuid()).unwrap(),
        to: to.to_vec(),
        cc: cc.to_vec(),
        subject: subject.to_owned(),
        body: body.to_owned(),
        attachments: Vec::new(),
    }
}

pub fn attach(mut mail: NewMail, name: &str, reference: &str) -> NewMail {
    mail.attachments
        .push(Attachment::new(name, DocumentRef::new(reference)));
    mail
}

pub fn to_incoming(entry: &TurnEntry) -> Option<IncomingEntry> {
    match entry {
        TurnEntry::HumanMessage(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::AgentMail(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::RequestAccepted(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::QueuePosition(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::TaskStarted(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::TaskCompleted(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::TaskFailed(entry) => Some(IncomingEntry::from(entry.clone())),
        TurnEntry::RuntimeNotice(entry) => Some(IncomingEntry::from(entry.clone())),
        _ => None,
    }
}

pub struct Held {
    pub turn: TurnId,
    pub in_ai_turn: bool,
    pub id: EntryId,
    pub incoming: Option<IncomingEntry>,
}

pub fn held(conversation: &Conversation) -> Vec<Held> {
    conversation
        .entries()
        .map(|placed| {
            let (in_ai_turn, incoming) = match placed.entry() {
                EntryView::Incoming(entry) => (false, Some((*entry).clone())),
                EntryView::Turn(entry) => (true, to_incoming(entry)),
                _ => (false, None),
            };
            Held {
                turn: placed.turn(),
                in_ai_turn,
                id: placed.entry().id(),
                incoming,
            }
        })
        .collect()
}

pub fn count_of(conversation: &Conversation, id: EntryId) -> usize {
    held(conversation).iter().filter(|h| h.id == id).count()
}

pub fn position_of(conversation: &Conversation, id: EntryId) -> Option<usize> {
    held(conversation).iter().position(|h| h.id == id)
}

pub fn incoming_with(conversation: &Conversation, id: EntryId) -> Option<IncomingEntry> {
    held(conversation)
        .into_iter()
        .find(|h| h.id == id)
        .and_then(|h| h.incoming)
}

pub fn block_text(block: &PlainBlock) -> String {
    match block {
        PlainBlock::Text(text) => text.text().as_str().to_owned(),
        other => format!("{other:?}"),
    }
}

pub struct Outcome {
    pub tool: String,
    pub is_error: bool,
    pub blocks: Vec<String>,
}

impl Outcome {
    pub fn text(&self) -> String {
        self.blocks.join("\n")
    }
}

pub fn outcomes(conversation: &Conversation) -> Vec<Outcome> {
    ai_turns(conversation)
        .into_iter()
        .flat_map(results)
        .flat_map(|entry| entry.results())
        .map(|result| Outcome {
            tool: result.tool_name().as_str().to_owned(),
            is_error: result.is_error(),
            blocks: result.content().iter().map(block_text).collect(),
        })
        .collect()
}

pub fn mail_text(entry: &IncomingEntry) -> String {
    match entry {
        IncomingEntry::AgentMail(mail) => {
            let blocks: Vec<String> = mail.content().iter().map(block_text).collect();
            assert_eq!(blocks.len(), 1, "an agent mail holds one text block");
            blocks.into_iter().next().unwrap()
        }
        IncomingEntry::RuntimeNotice(notice) => notice.text().to_owned(),
        other => panic!("not a mail entry: {other:?}"),
    }
}

pub fn mail_id_text(id: MailId) -> String {
    id.to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    AddEntry(EntryId),
    AddTurn(Vec<EntryId>),
    Mark(EntryId),
    Close,
    Other,
}

pub fn steps(record: &br_agentic_contract::memory::Record) -> Vec<Step> {
    use br_agentic_contract::memory::Call;
    record
        .calls()
        .into_iter()
        .map(|call| match call {
            Call::AddEntry { entry, .. } => Step::AddEntry(entry.id()),
            Call::AddInputTurn { turn, .. } => {
                Step::AddTurn(turn.entries().iter().map(IncomingEntry::id).collect())
            }
            Call::InboxMarkRead { item, .. } => Step::Mark(item),
            Call::CloseTurn { .. } => Step::Close,
            _ => Step::Other,
        })
        .collect()
}

pub fn index_of(steps: &[Step], wanted: &Step) -> usize {
    steps
        .iter()
        .position(|step| step == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} not in {steps:?}"))
}

pub fn plain(rig: &Rig, model: &Arc<Scripted>, author_name: &str) -> ReactAgent<Doc> {
    let inbox: Arc<dyn InboxPort> = rig.inbox.clone();
    build(rig, Some(inbox), model, author_name)
}
