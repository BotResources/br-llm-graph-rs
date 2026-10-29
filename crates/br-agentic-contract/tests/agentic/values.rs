use br_agentic_contract::memory::{FixedThreadPort, FixedToolsPort, MemoryAgentPort};
use br_agentic_contract::{
    AGENTIC, AgentLabel, AgentPort, AgenticNode, InboxEntry, Restart, Site, ThreadPort, ToolsPort,
    WebAccess,
};
use br_graph_contract::{CancelView, CreateFuture, Instance, InstanceId, Node};
use br_llm_messages::{
    AgentOutput, IncomingEntry, RequestAccepted, RequestId, RuntimeNotice, TaskCompleted, TaskId,
};

use crate::common::*;

struct Dummy {
    inbox: bool,
}

impl Node<Doc> for Dummy {
    fn name(&self) -> &str {
        "dummy"
    }

    fn kind(&self) -> &str {
        AGENTIC
    }

    fn subkind(&self) -> &str {
        "react_agent"
    }

    fn create(&self, _cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Doc> {
        Box::pin(async move { Ok(Instance::<Doc>::channel(id).0) })
    }
}

impl AgenticNode<Doc> for Dummy {
    fn has_inbox(&self) -> bool {
        self.inbox
    }

    fn restart(&self) -> Restart {
        Restart::Resets
    }
}

#[test]
fn given_an_agentic_node_when_it_is_asked_then_it_tells_its_kind_whether_it_has_an_inbox_and_what_a_restart_does()
 {
    let with: &dyn AgenticNode<Doc> = &Dummy { inbox: true };
    let without: &dyn AgenticNode<Doc> = &Dummy { inbox: false };

    assert_eq!(with.kind(), "agentic");
    assert!(with.has_inbox() && !without.has_inbox());
    assert_eq!(with.restart(), Restart::Resets);
}

#[test]
fn given_a_blank_text_when_an_agent_label_or_a_site_is_made_then_it_is_refused() {
    assert!(AgentLabel::new("  ").is_err());
    assert!(AgentLabel::new("critic").is_ok());
    assert!(Site::new("").is_err());
    assert!(Site::new("example.org").is_ok());
}

#[test]
fn given_the_three_web_accesses_when_they_are_asked_for_their_sites_then_only_a_whitelist_names_sites()
 {
    let site = Site::new("example.org").unwrap();

    assert_eq!(WebAccess::Whitelist(vec![site.clone()]).sites(), &[site]);
    assert!(WebAccess::Full.sites().is_empty());
    assert!(WebAccess::None.sites().is_empty());
    assert!(WebAccess::None.reaches_nothing());
    assert!(!WebAccess::Full.reaches_nothing());
}

#[test]
fn given_the_entries_an_inbox_may_give_when_an_inbox_entry_is_made_then_a_human_message_a_notification_and_a_notice_pass_and_agent_output_is_refused()
 {
    let notice = IncomingEntry::from(RuntimeNotice::new(entry_id(), "n").unwrap());
    let accepted = IncomingEntry::from(RequestAccepted::new(
        entry_id(),
        RequestId::from_uuid(uuid()).unwrap(),
    ));
    let completed = IncomingEntry::from(TaskCompleted::new(
        entry_id(),
        TaskId::from_uuid(uuid()).unwrap(),
    ));
    let output = IncomingEntry::from(
        AgentOutput::new(entry_id(), agent_id(), author("a"), vec![text_block("x")]).unwrap(),
    );

    assert!(InboxEntry::new(notice).is_ok());
    assert!(InboxEntry::new(accepted).is_ok());
    assert!(InboxEntry::new(completed).is_ok());
    assert!(InboxEntry::new(output).is_err());
    assert!(InboxEntry::new(human("h").into_entry()).is_ok());
}

#[test]
fn given_a_mail_with_no_recipient_in_to_or_a_repeated_recipient_when_it_is_checked_then_a_reason_is_given()
 {
    let a = agent_id();

    assert!(new_mail(&[], &[a], "s").refusal().is_some());
    assert!(new_mail(&[a, a], &[], "s").refusal().is_some());
    assert!(new_mail(&[a], &[a], "s").refusal().is_some());
    assert!(new_mail(&[a], &[agent_id()], "s").refusal().is_none());
}

#[tokio::test]
async fn given_the_in_memory_agent_port_when_it_is_asked_twice_for_a_label_then_it_gives_the_same_identity_and_another_label_gives_another()
 {
    let hub = br_agentic_contract::memory::MemoryHub::new();
    let port = MemoryAgentPort::<Doc>::new(hub)
        .with_author(AgentLabel::new("critic").unwrap(), author("Critic"));
    let snapshot = Doc { n: 0 };
    let generator = AgentLabel::new("generator").unwrap();
    let critic = AgentLabel::new("critic").unwrap();

    let first = port
        .agent(&snapshot, instance_id(), &generator)
        .await
        .unwrap();
    let again = port
        .agent(&snapshot, instance_id(), &generator)
        .await
        .unwrap();
    let other = port.agent(&snapshot, instance_id(), &critic).await.unwrap();

    assert_eq!(first.id, again.id);
    assert_ne!(first.id, other.id);
    assert_eq!(other.author, Some(author("Critic")));
}

#[tokio::test]
async fn given_the_simple_in_memory_ports_when_they_are_asked_then_they_give_what_the_test_set() {
    let world = World::new();
    let threads = FixedThreadPort::new(world.thread);
    let tools = FixedToolsPort::new(Vec::new());
    let snapshot = Doc { n: 0 };

    assert_eq!(
        ThreadPort::<Doc>::thread(&threads, &snapshot)
            .await
            .unwrap(),
        world.thread
    );
    assert!(
        ToolsPort::<Doc>::tools(&tools, &snapshot, instance_id())
            .await
            .unwrap()
            .is_empty()
    );
}
