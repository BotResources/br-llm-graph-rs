use br_agentic_contract::memory::{MemoryInbox, MemoryThreadAccess, Record};
use br_agentic_contract::{
    AiTurnRef, Boundary, InboxContext, InboxEntry, MailItem, NewMail, Queue, Session, ThreadAccess,
    ThreadRef, result, store_at_boundary,
};
use br_graph_contract::{Event, Instance};
use br_llm_messages::{
    AgentId, AgentMail, Author, BlockId, ClosureReason, Conversation, ConversationId, EntryId,
    Error, IncomingEntry, InputTurn, ModelResponse, PlainBlock, PlainText, RuntimeToolResults,
    TextBlock, TurnClosure, TurnId,
};

use crate::common::*;
use crate::forgetful::ForgetfulThreadAccess;

struct Script {
    thread: ThreadRef,
    conversation: ConversationId,
    agent: AgentId,
    author: Author,
    trigger: InboxEntry,
    second: InboxEntry,
    late: InboxEntry,
    mail: NewMail,
    sender: AgentId,
    turns: [TurnId; 5],
    entries: [EntryId; 6],
    mail_block: BlockId,
    call_response: ModelResponse,
    results: RuntimeToolResults,
    final_response: ModelResponse,
}

impl Script {
    fn new() -> Self {
        let entries = [(); 6].map(|_| entry_id());
        let call = block_id();
        let agent = agent_id();
        Self {
            thread: thread_ref(),
            conversation: conversation_id(),
            agent,
            author: author("agent"),
            trigger: human("trigger"),
            second: human("second"),
            late: human("late"),
            mail: new_mail(&[agent], &[], "report"),
            sender: agent_id(),
            turns: [(); 5].map(|_| turn_id()),
            mail_block: block_id(),
            call_response: response_call(entries[0], call, "lookup"),
            results: tool_results(entries[1], call, "lookup", "found"),
            final_response: response_text(entries[2], "done"),
            entries,
        }
    }

    fn ai(&self, id: TurnId) -> AiTurnRef {
        AiTurnRef {
            id,
            agent: self.agent,
            author: self.author.clone(),
        }
    }

    fn mail_entry(&self) -> impl Fn(&MailItem) -> Result<IncomingEntry, Error> + Sync + use<> {
        let block = self.mail_block;
        move |item| match item {
            MailItem::To {
                id,
                sender,
                author,
                source,
                subject,
                ..
            } => AgentMail::new(
                id.entry_id(),
                *sender,
                author.clone(),
                *source,
                vec![PlainBlock::Text(TextBlock::new(block, subject)?)],
            )
            .map(IncomingEntry::from),
            MailItem::Cc { .. } => Err(PlainText::new(" ").unwrap_err()),
        }
    }

    fn context(&self, context: &InboxContext) -> InboxContext {
        InboxContext {
            agent: self.agent,
            thread: self.thread,
            ..*context
        }
    }
}

struct Played {
    conversation: Conversation,
    reports: Vec<Boundary>,
    ended_with_result: bool,
    open_at_the_end: usize,
}

async fn play(script: &Script, access: &dyn ThreadAccess, inbox: &MemoryInbox) -> Played {
    let context = script.context(&InboxContext {
        instance: instance_id(),
        thread: script.thread,
        agent: script.agent,
    });
    inbox.deliver_entry(script.agent, script.second.clone());
    inbox
        .deliver_mail(script.sender, script.mail.clone())
        .unwrap();
    let mut session = Session::start(access, script.thread).await.unwrap();
    session
        .add_input_turn(
            InputTurn::new(script.turns[0], vec![script.trigger.clone().into_entry()]).unwrap(),
        )
        .await
        .unwrap();
    let turn = script.ai(script.turns[1]);
    let _stream = session
        .open_response(&turn, script.entries[0])
        .await
        .unwrap();
    session
        .complete_response(&turn, script.call_response.clone())
        .await
        .unwrap();
    let mut fragments = session.open_result(&turn, script.entries[1]).await.unwrap();
    fragments.send("found".to_owned()).await;
    session
        .complete_result(&turn, script.results.clone())
        .await
        .unwrap();

    let mut reports = Vec::new();
    let mut queue = Queue::new();
    queue.read(inbox, &context, limit(10)).await.unwrap();
    reports.push(
        store_at_boundary(
            &mut session,
            inbox,
            &context,
            &mut queue,
            &script.mail_entry(),
            script.turns[2],
        )
        .await
        .unwrap(),
    );
    session
        .complete_response(&turn, script.final_response.clone())
        .await
        .unwrap();
    session
        .close_turn(
            &turn,
            TurnClosure::new(script.entries[3], ClosureReason::completed()),
        )
        .await
        .unwrap();
    inbox.deliver_entry(script.agent, script.late.clone());
    queue.read(inbox, &context, limit(10)).await.unwrap();
    reports.push(
        store_at_boundary(
            &mut session,
            inbox,
            &context,
            &mut queue,
            &script.mail_entry(),
            script.turns[3],
        )
        .await
        .unwrap(),
    );
    let next = script.ai(script.turns[4]);
    let _left_open = session
        .open_response(&next, script.entries[4])
        .await
        .unwrap();

    let (_instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let event = result::<Doc>(&reader, &mut session, 5).await;
    Played {
        conversation: session.conversation().clone(),
        reports,
        ended_with_result: matches!(event, Event::Result(5)),
        open_at_the_end: session.open_objects(),
    }
}

#[tokio::test]
async fn given_a_thread_access_that_discards_everything_it_receives_when_an_instance_runs_a_whole_session_then_the_copy_the_boundary_reports_and_the_outcome_are_those_of_a_thread_access_that_keeps_everything()
 {
    let script = Script::new();
    let record = Record::new();
    let keeping = MemoryThreadAccess::new(
        &record,
        script.thread,
        Conversation::new(script.conversation),
    );
    let forgetful = ForgetfulThreadAccess::new(script.conversation);

    let kept = play(&script, &keeping, &MemoryInbox::new(&record)).await;
    let forgotten = play(&script, &forgetful, &MemoryInbox::new(&Record::new())).await;

    assert_eq!(forgotten.conversation, kept.conversation);
    assert_eq!(forgotten.reports, kept.reports);
    assert!(forgotten.ended_with_result && kept.ended_with_result);
    assert_eq!(forgotten.open_at_the_end, kept.open_at_the_end);
    assert_eq!(forgetful.reads(), 1);
    assert_eq!(forgotten.reports[0].stored.len(), 2);
    assert_eq!(forgotten.reports[1].stored.len(), 1);
    assert!(forgotten.conversation.entry(&script.entries[2]).is_ok());
    assert_eq!(forgotten.conversation.turns().len(), 3);
}
