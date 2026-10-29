use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{CancelToken, Event, EventEnd, Input, InputEnd, Node, State};

use super::{Doc, Recorded, doc, instance_id};

pub struct Driver {
    pub input: InputEnd<Doc>,
    pub events: EventEnd<Doc>,
    pub cancel: CancelToken,
}

impl Driver {
    pub async fn start(node: &dyn Node<Doc>) -> Self {
        let cancel = CancelToken::new();
        let instance = node
            .create(cancel.view(), instance_id())
            .await
            .expect("an instance");
        let (input, events) = instance.into_parts();
        Self {
            input,
            events,
            cancel,
        }
    }

    pub fn give(&self) {
        let state = State::create(doc()).unwrap();
        self.input.give(Input::new(Arc::new(state))).unwrap();
    }

    pub async fn run(node: &dyn Node<Doc>) -> (Self, Event<Doc>) {
        let mut driver = Self::start(node).await;
        driver.give();
        let event = driver.event().await;
        (driver, event)
    }

    pub async fn event(&mut self) -> Event<Doc> {
        tokio::time::timeout(Duration::from_secs(3600), self.events.recv())
            .await
            .expect("an event")
            .expect("an event")
    }

    pub async fn result(node: &dyn Node<Doc>) -> Recorded {
        match Self::run(node).await.1 {
            Event::Result(update) => update,
            other => panic!("expected a Result, got {other:?}"),
        }
    }

    pub async fn failure(node: &dyn Node<Doc>) -> String {
        match Self::run(node).await.1 {
            Event::Failed(cause) => chain(&*cause),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}

fn chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        parts.push(cause.to_string());
        source = cause.source();
    }
    parts.join(": ")
}
