use br_llm_messages::AgentId;

use super::*;

async fn deliver_to_both(world: &World, second: AgentId) -> (EntryId, Queue, Queue) {
    let mail = new_mail(&[world.agent, second], &[], "shared");
    let id = mail.id.entry_id();
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let first = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let other = world
        .inbox
        .items(&world.context_of(second), None, limit(5))
        .await
        .unwrap();
    (id, queue_of(&first.items), queue_of(&other.items))
}

#[tokio::test]
async fn given_two_agents_on_one_thread_when_the_second_copy_is_read_after_the_first_store_then_the_second_boundary_skips_and_marks_the_mail()
 {
    let world = World::new();
    let second = agent_id();
    let (id, mut first_queue, mut second_queue) = deliver_to_both(&world, second).await;
    let mut first = world.session().await;
    run(&mut first, &world, &mut first_queue).await.unwrap();
    let mut late = world.session().await;

    let done = store_at_boundary(
        &mut late,
        &world.inbox,
        &world.context_of(second),
        &mut second_queue,
        &mail_entry,
        turn_id(),
    )
    .await
    .unwrap();

    assert_eq!(done.skipped, vec![id]);
    assert!(done.stored.is_empty());
    assert!(world.inbox.is_read(second, id));
}

#[tokio::test]
async fn given_two_agents_on_one_thread_when_the_second_copy_was_read_before_the_first_store_then_the_second_boundary_faults_and_marks_nothing()
 {
    let world = World::new();
    let second = agent_id();
    let (id, mut first_queue, mut second_queue) = deliver_to_both(&world, second).await;
    let mut stale = world.session().await;
    let mut first = world.session().await;
    run(&mut first, &world, &mut first_queue).await.unwrap();

    let error = store_at_boundary(
        &mut stale,
        &world.inbox,
        &world.context_of(second),
        &mut second_queue,
        &mail_entry,
        turn_id(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error.fault, SessionError::Port(_)));
    assert!(!world.inbox.is_read(second, id));
}
