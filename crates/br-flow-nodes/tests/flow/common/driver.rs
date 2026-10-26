use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{
    CancelToken, Event, EventEnd, Fields, Input, InputClosed, InputEnd, InstanceId, Node, State,
};
use uuid::Uuid;

pub struct Driver<F: Fields> {
    pub input: InputEnd<F>,
    pub events: EventEnd<F>,
    pub cancel: CancelToken,
}

impl<F: Fields> Driver<F> {
    pub async fn start(node: &dyn Node<F>) -> Self {
        let cancel = CancelToken::new();
        let id = InstanceId::new(Uuid::now_v7()).expect("a v7 id");
        let instance = node.create(cancel.view(), id).await.expect("an instance");
        assert_eq!(instance.id(), id);
        let (input, events) = instance.into_parts();
        Self {
            input,
            events,
            cancel,
        }
    }

    pub fn give(&self, state: State<F>) -> Result<(), InputClosed> {
        self.input.give(Input::new(Arc::new(state)))
    }

    pub async fn event(&mut self) -> Event<F> {
        self.events.recv().await.expect("an event")
    }

    pub async fn update(&mut self) -> F::Update {
        match self.event().await {
            Event::Result(update) => update,
            other => panic!("expected a Result, got {other:?}"),
        }
    }

    pub async fn silent(&mut self) {
        let waited = tokio::time::timeout(Duration::from_secs(3600), self.events.recv()).await;
        assert!(waited.is_err(), "expected silence, got {waited:?}");
    }
}
