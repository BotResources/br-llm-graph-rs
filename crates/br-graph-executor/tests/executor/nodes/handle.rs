use br_graph_contract::{CancelView, Dropped, Event, EventSender, InputReader, InstanceId};

use crate::host::within;
use crate::kv::{Change, Kv};

use super::test_node::TestError;

pub struct Handle {
    pub node: String,
    pub id: InstanceId,
    pub token: CancelView,
    reader: InputReader<Kv>,
    sender: Option<EventSender<Kv>>,
    dropped: Dropped,
}

impl Handle {
    pub(super) fn new(
        node: String,
        id: InstanceId,
        token: CancelView,
        reader: InputReader<Kv>,
        sender: EventSender<Kv>,
        dropped: Dropped,
    ) -> Self {
        Self {
            node,
            id,
            token,
            reader,
            sender: Some(sender),
            dropped,
        }
    }

    pub async fn next_input(&mut self) -> Kv {
        within(self.reader.next()).await.unwrap().fields().clone()
    }

    pub fn accepted_inputs(&mut self) -> Vec<Kv> {
        self.reader
            .drain()
            .iter()
            .map(|input| input.fields().clone())
            .collect()
    }

    pub fn close_input(&self) {
        self.reader.close();
    }

    fn finish(&mut self, event: Event<Kv>) {
        self.sender.take().unwrap().finish(event);
    }

    pub fn result(&mut self, update: Change) {
        self.finish(Event::Result(update));
    }

    pub fn failed(&mut self, message: &str) {
        self.finish(Event::failed(TestError(message.to_owned())));
    }

    pub fn refused(&mut self, reason: &str) {
        self.finish(Event::refused(reason));
    }

    pub fn cancelled(&mut self) {
        self.finish(Event::Cancelled);
    }

    pub fn vanish(&mut self) {
        self.sender.take();
    }

    pub fn panic_in_task(&mut self) {
        let sender = self.sender.take();
        tokio::spawn(async move {
            let _sender = sender;
            panic!("the work of the node panics");
        });
    }

    pub async fn wait_dropped(&mut self) {
        within(&mut self.dropped).await;
    }

    pub async fn is_dropped(&mut self) -> bool {
        tokio::select! {
            biased;
            () = &mut self.dropped => true,
            () = std::future::ready(()) => false,
        }
    }
}
