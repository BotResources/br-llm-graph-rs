use br_agentic_harness_contract::{OpenFault, PortError, SandboxPort, Seed};

use crate::common::*;
use crate::scenario::*;

fn name(fault: &PortError) -> &'static str {
    match OpenFault::of(fault) {
        Some(OpenFault::ImageUnavailable) => "unavailable",
        Some(OpenFault::ImageFailing) => "failing",
        None => "neither",
    }
}

#[tokio::test]
async fn given_a_test_that_makes_the_next_open_fail_with_each_error_when_the_caller_tests_the_error_then_one_test_tells_the_two_apart_and_both_are_faults()
-> Res {
    let world = World::new()?;
    let mut answers = Vec::new();
    for fault in [OpenFault::ImageUnavailable, OpenFault::ImageFailing] {
        world.sandbox.fail_next_open(fault);
        let request = request_of(world.turn_ref(None)?, Seed::empty())?;

        let Err(error) = world.sandbox.open(&request).await else {
            return Err("the open fails".into());
        };

        assert!(matches!(error, PortError::Fault(_)));
        assert!(!error.is_refusal());
        answers.push(name(&error));
    }

    assert_eq!(answers, vec!["unavailable", "failing"]);
    Ok(())
}

#[test]
fn given_a_refusal_a_fault_of_the_provider_and_the_two_named_errors_when_the_typed_test_is_applied_then_only_the_two_named_errors_are_told()
 {
    let refusal = PortError::refused("ImageUnavailable");
    let fault = PortError::fault("ImageFailing");

    assert_eq!(name(&refusal), "neither");
    assert_eq!(name(&fault), "neither");
    assert_eq!(
        name(&PortError::from(OpenFault::ImageUnavailable)),
        "unavailable"
    );
    assert_eq!(name(&PortError::from(OpenFault::ImageFailing)), "failing");
}

#[test]
fn given_the_errors_of_open_when_they_are_made_into_port_errors_then_the_caller_gets_two_different_values_and_a_shortage_of_capacity_has_no_value_of_its_own()
 {
    let all = [OpenFault::ImageUnavailable, OpenFault::ImageFailing];

    let told: Vec<Option<OpenFault>> = all
        .iter()
        .map(|fault| OpenFault::of(&PortError::from(*fault)))
        .collect();

    assert_eq!(told, vec![Some(all[0]), Some(all[1])]);
    assert_ne!(told[0], told[1]);
}

#[tokio::test]
async fn given_an_open_that_failed_when_the_same_instance_opens_again_then_nothing_was_claimed_and_the_next_open_succeeds()
-> Res {
    let world = World::new()?;
    let request = request_of(world.turn_ref(None)?, Seed::empty())?;
    world.sandbox.fail_next_open(OpenFault::ImageUnavailable);

    let failed = world.sandbox.open(&request).await;
    let again = world.sandbox.open(&request).await;

    assert!(failed.is_err());
    assert!(again.is_ok());
    Ok(())
}
