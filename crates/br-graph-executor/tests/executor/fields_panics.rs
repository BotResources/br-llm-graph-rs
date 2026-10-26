use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{
    CancelToken, CancelView, Cause, CreateError, CreateFuture, Edge, Event, Fields, GraphBuilder,
    Instance, InstanceId, Node, PROGRAMMATIC, Reason, State, Target,
};
use br_graph_executor::{Outcome, ResumeError, RunConfig, resume, start};
use serde::{Deserialize, Serialize};

use crate::graphs::*;
use crate::host::*;

thread_local! {
    static CLONING_PANICS: Cell<bool> = const { Cell::new(false) };
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Fragile(u32);

impl Clone for Fragile {
    fn clone(&self) -> Self {
        if CLONING_PANICS.with(Cell::get) {
            panic!("cloning panics");
        }
        Self(self.0)
    }
}

impl Fields for Fragile {
    type Update = u32;

    fn empty_update() -> u32 {
        0
    }

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, update: u32) -> Result<(), Reason> {
        self.0 = update;
        CLONING_PANICS.with(|flag| flag.set(true));
        Ok(())
    }
}

struct OneResult;

impl Node<Fragile> for OneResult {
    fn name(&self) -> &str {
        "a"
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "worker"
    }

    fn create(&self, _cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Fragile> {
        Box::pin(async move {
            let (instance, mut reader, sender) = Instance::<Fragile>::channel(id);
            tokio::spawn(async move {
                let _ = reader.next().await;
                sender.finish(Event::Result(7));
            });
            Ok::<_, CreateError>(instance)
        })
    }
}

struct ToEnd;

impl Edge<Fragile> for ToEnd {
    fn name(&self) -> &str {
        "after_a"
    }

    fn source(&self) -> &str {
        "a"
    }

    fn targets(&self) -> &[Target] {
        &[Target::End]
    }

    fn evaluate(&self, _state: &State<Fragile>) -> Result<Vec<Target>, Cause> {
        Ok(vec![Target::End])
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_state_panicking_in_validation_when_resumed_then_it_is_refused_as_state_rejected() {
    let rig = single("a");
    let checkpoint = handwritten_checkpoint(
        &rig,
        r#"{"values":{"panic_on_validate":1},"pad":""}"#,
        &["a"],
    );
    let config = RunConfig::new(CancelToken::new(), GRACE);
    let refused = within(resume(rig.graph.clone(), checkpoint, BIG, config)).await;
    match refused {
        Err(ResumeError::StateRejected { reason }) => {
            assert_eq!(reason, "validation panics");
        }
        other => panic!("expected StateRejected, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_final_state_held_by_the_run_alone_when_the_run_ends_then_it_is_moved_out_without_cloning()
 {
    CLONING_PANICS.with(|flag| flag.set(false));
    let graph = GraphBuilder::<Fragile>::new("1")
        .node(Arc::new(OneResult))
        .edge(Arc::new(ToEnd))
        .entry("a")
        .build()
        .unwrap();
    let initial = State::create(Fragile(0)).unwrap();
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let outcome = within(start(
        Arc::new(graph),
        initial,
        vec!["a".to_owned()],
        config,
    ))
    .await;
    CLONING_PANICS.with(|flag| flag.set(false));
    match outcome.unwrap() {
        Outcome::RunEnded { state } => assert_eq!(state.into_fields(), Fragile(7)),
        other => panic!("expected RunEnded, got {other:?}"),
    }
}
