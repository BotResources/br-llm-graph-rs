use std::sync::Arc;
use std::time::Duration;

use br_flow_nodes::MapReduceError;
use br_graph_contract::{
    CancelToken, Event, EventEnd, EventEndError, Fields, Input, InputClosed, InputEnd, InstanceId,
    Node, State,
};
use uuid::Uuid;

use super::probe::{Log, Mark, Witness};
use super::{BOUND, Report};

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
        tokio::time::timeout(BOUND, self.events.recv())
            .await
            .expect("an event within the bound")
            .expect("an event")
    }

    pub async fn event_marked(&mut self, log: &Arc<Log>) -> Event<F> {
        let received = Witness::new(Box::pin(self.events.recv()), log, Mark::Delivered);
        tokio::time::timeout(BOUND, received)
            .await
            .expect("an event within the bound")
            .expect("an event")
    }

    pub async fn ended(&mut self) -> EventEndError {
        tokio::time::timeout(BOUND, self.events.recv())
            .await
            .expect("the end of the events within the bound")
            .expect_err("no further event")
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

impl Driver<Report> {
    pub async fn failure(&mut self) -> MapReduceError {
        failed(self.event().await)
    }

    pub async fn failure_marked(&mut self, log: &Arc<Log>) -> MapReduceError {
        failed(self.event_marked(log).await)
    }
}

fn failed(event: Event<Report>) -> MapReduceError {
    match event {
        Event::Failed(cause) => *cause
            .downcast::<MapReduceError>()
            .expect("the cause is a MapReduceError"),
        other => panic!("expected Failed, got {other:?}"),
    }
}
