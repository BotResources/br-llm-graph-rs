use br_graph_contract::{DEFAULT_STATE_LIMIT, InstanceId, PROGRAMMATIC, Reason, Target};
use uuid::Uuid;

#[test]
fn given_a_uuid_v7_when_made_an_instance_id_then_it_is_kept() {
    let uuid = Uuid::parse_str("0190a5f0-7c3e-7b4a-8c1d-2e3f4a5b6c7d").unwrap();
    let id = InstanceId::new(uuid).unwrap();
    assert_eq!(id.as_uuid(), uuid);
    assert_eq!(id.to_string(), uuid.to_string());
    assert_eq!(Uuid::from(id), uuid);
}

#[test]
fn given_other_uuid_versions_when_made_an_instance_id_then_they_are_refused() {
    let v4 = Uuid::parse_str("5e1c3f7a-2b9d-4c6e-8a1f-0d2b3c4e5f60").unwrap();
    let nil = Uuid::nil();
    let v7_wrong_variant = Uuid::parse_str("0190a5f0-7c3e-7b4a-cc1d-2e3f4a5b6c7d").unwrap();
    for uuid in [v4, nil, v7_wrong_variant] {
        let error = InstanceId::new(uuid).unwrap_err();
        assert!(error.to_string().contains(&uuid.to_string()));
        assert!(InstanceId::try_from(uuid).is_err());
    }
}

#[test]
fn given_instance_ids_when_compared_then_they_follow_the_uuid() {
    let low = crate::common::id(1);
    let high = crate::common::id(2);
    assert!(low < high);
    assert_eq!(low, crate::common::id(1));
}

#[test]
fn given_the_end_name_when_made_a_target_then_it_is_end_and_never_a_node() {
    assert_eq!(Target::from("END"), Target::End);
    assert_eq!(Target::from(String::from("END")), Target::END);
    assert_eq!(Target::from("review"), Target::node("review"));
    assert!(Target::End.is_end());
    assert!(!Target::node("review").is_end());
    assert_eq!(Target::End.to_string(), "END");
    assert_eq!(Target::End.as_str(), "END");
}

#[test]
fn given_the_end_name_when_made_through_the_node_constructor_then_it_is_end_like_every_other_spelling()
 {
    assert_eq!(Target::node("END"), Target::End);
    assert_eq!(Target::node(String::from("END")), Target::End);
    let from_json: Target = serde_json::from_str(r#""END""#).unwrap();
    assert_eq!(from_json, Target::node("END"));
    assert_eq!(Target::node("END").as_str(), Target::from("END").as_str());
    assert_ne!(Target::node("end"), Target::End);
}

#[test]
fn given_targets_when_serialized_then_end_is_the_string_end() {
    let json = serde_json::to_string(&vec![Target::node("review"), Target::End]).unwrap();
    assert_eq!(json, r#"["review","END"]"#);
    let back: Vec<Target> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, vec![Target::node("review"), Target::End]);
}

#[test]
fn given_a_reason_when_converted_then_it_keeps_its_text() {
    let reason = Reason::from("no data");
    assert_eq!(reason.as_str(), "no data");
    assert_eq!(reason.to_string(), "no data");
    assert_eq!(Reason::from(String::from("no data")), reason);
    assert_eq!(reason.clone().into_string(), "no data");
    let as_ref: &str = reason.as_ref();
    assert_eq!(as_ref, "no data");
}

#[test]
fn given_the_constants_when_read_then_they_match_the_contract() {
    assert_eq!(PROGRAMMATIC, "programmatic");
    assert_eq!(DEFAULT_STATE_LIMIT.get(), 131_072);
}
