use std::sync::Arc;

use br_agentic_contract::memory::{Call, MemoryInbox, MemoryThreadAccess, Record};
use br_agentic_contract::{
    AiTurnRef, Boundary, InboxContext, InboxEntry, Queue, Session, ThreadRef, store_at_boundary,
};
use br_llm_messages::{AgentId, Conversation, EntryId};

use crate::common::*;

struct Agent {
    id: AgentId,
    item: InboxEntry,
}

async fn one_instance(
    access: &MemoryThreadAccess,
    inbox: &MemoryInbox,
    thread: ThreadRef,
    agent: &Agent,
) -> Boundary {
    let context = InboxContext {
        instance: instance_id(),
        thread,
        agent: agent.id,
    };
    let mut session = Session::start(access, thread).await.unwrap();
    let turn = AiTurnRef {
        id: turn_id(),
        agent: agent.id,
        author: author("agent"),
    };
    let entry = entry_id();
    let _stream = session.open_response(&turn, entry).await.unwrap();
    tokio::task::yield_now().await;
    session
        .complete_response(&turn, response_text(entry, "working"))
        .await
        .unwrap();
    tokio::task::yield_now().await;
    let mut queue = Queue::new();
    queue.read(inbox, &context, limit(10)).await.unwrap();
    tokio::task::yield_now().await;
    store_at_boundary(
        &mut session,
        inbox,
        &context,
        &mut queue,
        &mail_entry,
        turn_id(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn given_two_instances_of_two_agents_when_they_run_at_the_same_time_over_one_thread_access_and_one_inbox_then_each_stores_and_marks_only_its_own_items_and_the_record_holds_every_call()
 {
    let record = Record::new();
    let thread = thread_ref();
    let access = Arc::new(MemoryThreadAccess::new(
        &record,
        thread,
        Conversation::new(conversation_id()),
    ));
    let inbox = Arc::new(MemoryInbox::new(&record));
    let agents = [(); 2].map(|_| Agent {
        id: agent_id(),
        item: human("for me"),
    });
    for agent in &agents {
        inbox.deliver_entry(agent.id, agent.item.clone());
    }

    let (first, second) = tokio::join!(
        one_instance(&access, &inbox, thread, &agents[0]),
        one_instance(&access, &inbox, thread, &agents[1]),
    );

    for (agent, other, report) in [
        (&agents[0], &agents[1], first),
        (&agents[1], &agents[0], second),
    ] {
        assert_eq!(report.stored, vec![agent.item.id()]);
        assert!(inbox.is_read(agent.id, agent.item.id()));
        assert!(!inbox.is_read(agent.id, other.item.id()));
        assert!(access.conversation().entry(&agent.item.id()).is_ok());
    }
    let calls = record.calls();
    let count = |wanted: fn(&Call) -> bool| calls.iter().filter(|call| wanted(call)).count();
    assert_eq!(count(|call| matches!(call, Call::Read { .. })), 2);
    assert_eq!(count(|call| matches!(call, Call::OpenResponse { .. })), 2);
    assert_eq!(count(|call| matches!(call, Call::InboxMarkRead { .. })), 2);
    let marked: Vec<EntryId> = calls
        .iter()
        .filter_map(|call| match call {
            Call::InboxMarkRead { item, .. } => Some(*item),
            _ => None,
        })
        .collect();
    assert!(marked.contains(&agents[0].item.id()) && marked.contains(&agents[1].item.id()));
}
