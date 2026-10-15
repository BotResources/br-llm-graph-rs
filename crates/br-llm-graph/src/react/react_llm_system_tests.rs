//! The system text of the model-call step: the text its `system` gives,
//! fixed, read from a key or computed from the agent's context, and none
//! when it is blank.

use crate::react::given::{Prompt, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_llm_tests::{ctx, llm, run_on, single};
use crate::react::test_support::*;
use crate::state::{ConfigKey, StateKey};

#[tokio::test]
async fn given_a_system_read_from_a_list_of_strings_when_llm_runs_then_its_items_joined_by_a_blank_line()
 {
    let client = mock(vec![text_step("done")]);
    let node = llm(&client, Prompt::from_state_list(&skey("enabled")));

    run_on(&single(node), seeded_state(), &ctx()).await.unwrap();

    assert_eq!(system_of(&requests(&client)[0]).unwrap(), "echo\n\ntally");
}

#[tokio::test]
async fn given_a_computed_system_when_llm_runs_then_it_reads_the_state_the_configuration_and_the_conversation()
 {
    let client = mock(vec![text_step("done")]);
    let mut reads = Reads::new();
    let enabled: StateKey<Vec<String>> = reads.state("enabled").unwrap();
    let base: ConfigKey<String> = reads.config("base").unwrap();
    let computed: Prompt<ReactAgent> = Prompt::from_fn(reads, move |ctx| {
        Ok(format!(
            "{} Tools: {}. Entries: {}.",
            ctx.config(&base)?,
            ctx.get(&enabled)?.len(),
            ctx.conversation().entries().len()
        ))
    });

    run_on(&single(llm(&client, computed)), seeded_state(), &ctx())
        .await
        .unwrap();

    assert_eq!(
        system_of(&requests(&client)[0]).unwrap(),
        "You are a helpful agent. Tools: 2. Entries: 1."
    );
}

#[tokio::test]
async fn given_a_system_that_gives_a_blank_text_when_llm_runs_then_no_system_text() {
    for blank in ["", "  "] {
        let client = mock(vec![text_step("done")]);

        run_on(
            &single(llm(&client, Prompt::text(blank))),
            seeded_state(),
            &ctx(),
        )
        .await
        .unwrap();

        assert_eq!(system_of(&requests(&client)[0]), None, "{blank:?}");
    }
}
