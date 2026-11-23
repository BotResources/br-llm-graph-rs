use std::sync::Arc;

use br_agentic_harness_contract::memory::{MemoryMail, MemorySandbox, MemorySkills};
use br_agentic_harness_contract::{
    HarnessHistory, MailPort, RunLifecycle, SandboxPort, SkillsPort, ThreadAccess,
};

use crate::common::*;

#[test]
fn given_the_ports_when_they_are_used_as_trait_objects_then_they_compile_and_are_send_and_sync()
-> Res {
    let world = World::new()?;
    let mail: Arc<dyn MailPort> = Arc::new(MemoryMail::clone(&world.mail));
    let skills: Arc<dyn SkillsPort> = Arc::new(MemorySkills::new(&world.record));
    let sandbox: Arc<dyn SandboxPort> = Arc::new(MemorySandbox::clone(&world.sandbox));
    let history: Arc<dyn HarnessHistory> = Arc::new(world.history.clone());
    let node_view: &dyn ThreadAccess = &*history;
    let _lifecycle: Option<Arc<dyn RunLifecycle>> = None;

    fn is_send_sync<T: Send + Sync + ?Sized>(_: &T) {}
    is_send_sync(&*mail);
    is_send_sync(&*skills);
    is_send_sync(&*sandbox);
    is_send_sync(&*history);
    is_send_sync(node_view);
    Ok(())
}
