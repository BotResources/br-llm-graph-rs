use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{Event, Input, State};
use br_llm_messages::{ClosureReason, Conversation};

use crate::common::{Doc, Driver, ai_turns, closure, doc};

pub fn input_is_closed(driver: &Driver) -> bool {
    let state = State::create(doc()).unwrap();
    driver.input.give(Input::new(Arc::new(state))).is_err()
}

pub fn input_with(note: &str) -> Input<Doc> {
    let state = State::create(Doc {
        note: note.to_owned(),
    })
    .unwrap();
    Input::new(Arc::new(state))
}

pub async fn until(mut condition: impl FnMut() -> bool) {
    for _ in 0..1000 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("the condition never held");
}

pub fn closures_of(conversation: &Conversation) -> Vec<Option<ClosureReason>> {
    ai_turns(conversation).into_iter().map(closure).collect()
}

pub fn is_cancelled(event: &Event<Doc>) -> bool {
    matches!(event, Event::Cancelled)
}

pub fn is_failed(event: &Event<Doc>) -> bool {
    matches!(event, Event::Failed(_))
}
