use br_agentic_contract::{
    AgentLabel, InvalidLabel, InvalidSite, InvalidThreadRef, MailId, Site, ThreadRef, WebAccess,
};
use br_llm_messages::Uuid;

use crate::common::*;

fn v4() -> Uuid {
    Uuid::from_u128(0x0123_4567_89ab_4def_8123_4567_89ab_cdef)
}

#[test]
fn given_an_empty_text_when_an_agent_label_is_made_then_it_is_refused() {
    assert_eq!(AgentLabel::new(""), Err(InvalidLabel));
}

#[test]
fn given_a_non_empty_text_when_an_agent_label_is_made_then_it_keeps_the_text() {
    assert_eq!(AgentLabel::new("critic").unwrap().as_str(), "critic");
}

#[test]
fn given_a_uuid_that_is_not_a_v7_when_a_thread_reference_is_made_then_it_is_refused() {
    assert_eq!(ThreadRef::from_uuid(Uuid::nil()), Err(InvalidThreadRef));
    assert_eq!(ThreadRef::from_uuid(v4()), Err(InvalidThreadRef));
}

#[test]
fn given_a_uuid_v7_when_a_thread_reference_is_made_then_it_names_the_same_uuid() {
    let uuid = uuid();
    assert_eq!(ThreadRef::from_uuid(uuid).unwrap().as_uuid(), uuid);
}

#[test]
fn given_a_uuid_that_is_not_a_v7_when_a_mail_id_is_made_then_it_is_refused() {
    assert!(MailId::from_uuid(v4()).is_err());
    assert!(MailId::from_uuid(uuid()).is_ok());
}

#[test]
fn given_a_blank_text_when_a_site_is_made_then_it_is_refused() {
    assert_eq!(Site::new("  "), Err(InvalidSite));
    assert_eq!(Site::new(""), Err(InvalidSite));
}

#[test]
fn given_a_non_blank_text_when_a_site_is_made_then_the_library_gives_it_no_syntax_and_keeps_it() {
    assert_eq!(
        Site::new("*.example.org / anything").unwrap().as_str(),
        "*.example.org / anything"
    );
}

#[test]
fn given_an_empty_whitelist_when_the_web_access_is_built_then_it_is_valid_and_reaches_no_site() {
    let access = WebAccess::Whitelist(Vec::new());
    assert!(access.reaches_nothing());
    assert!(access.sites().is_empty());
}

#[test]
fn given_the_three_web_accesses_then_only_none_and_the_empty_whitelist_reach_nothing() {
    assert!(WebAccess::None.reaches_nothing());
    assert!(!WebAccess::Full.reaches_nothing());
    let one = WebAccess::Whitelist(vec![Site::new("example.org").unwrap()]);
    assert!(!one.reaches_nothing());
    assert_eq!(one.sites().len(), 1);
}
