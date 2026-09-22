use br_llm_messages::UserInput;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::value::Key;

pub enum Message {
    Input { key: Key, input: UserInput },
    Pause,
    Resume,
    Cancel,
}

pub struct Inbox {
    pub(crate) receiver: UnboundedReceiver<Message>,
}

#[derive(Clone)]
pub struct Sender {
    sender: UnboundedSender<Message>,
}

pub fn channel() -> (Sender, Inbox) {
    let (sender, receiver) = unbounded();
    (Sender { sender }, Inbox { receiver })
}

impl Sender {
    pub fn send(&self, key: Key, input: UserInput) {
        let _ = self.sender.unbounded_send(Message::Input { key, input });
    }

    pub fn pause(&self) {
        let _ = self.sender.unbounded_send(Message::Pause);
    }

    pub fn resume(&self) {
        let _ = self.sender.unbounded_send(Message::Resume);
    }

    pub fn cancel(&self) {
        let _ = self.sender.unbounded_send(Message::Cancel);
    }
}

impl Inbox {
    pub(crate) fn try_next(&mut self) -> Option<Message> {
        self.receiver.try_recv().ok()
    }
}
