use br_agentic_contract::WebAccess;
use br_agentic_harness_contract::{OpenRequest, Seed, Site};

use crate::common::*;
use crate::scenario::*;

#[test]
fn given_a_request_when_it_is_made_then_it_holds_the_instance_the_web_access_the_turn_reference_and_the_seed()
-> Res {
    let world = World::new()?;
    let parent = turn_id()?;
    let turn = world.turn_ref(Some(parent))?;
    let instance = instance_id()?;
    let web = WebAccess::Whitelist(vec![Site::new("example.org")?]);
    let seed = seed_of(&[("s.md", b"seed")])?;

    let request = OpenRequest::new(instance, Some(web.clone()), turn, seed.clone());

    assert_eq!(request.instance, instance);
    assert_eq!(request.web_access, Some(web));
    assert_eq!(request.turn, turn);
    assert_eq!(request.turn.thread, world.thread);
    assert_eq!(request.turn.agent, world.agent);
    assert_eq!(request.turn.parent, Some(parent));
    assert_eq!(request.seed, seed);
    let without = OpenRequest::new(instance, None, world.turn_ref(None)?, Seed::empty());
    assert_eq!(without.web_access, None);
    assert_eq!(without.turn.parent, None);
    Ok(())
}
