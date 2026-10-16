//! The control channel: how the host sends commands to a run or a session.

use br_llm_messages::UserInput;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::value::Key;

/// One command of the host, as the run or the session receives it.
pub(crate) enum Command {
    Input { key: Key, input: UserInput },
    Pause,
    Resume,
    Cancel,
}

/// The receiving end of a control channel, handed to `run` or to a
/// `Session`.
///
/// It is listened to while a superstep runs: an input is held and applied
/// after the node updates of that superstep, a pause is honoured at its end,
/// a cancel stops it at once. Once every `ControlSender` is dropped the
/// channel is closed: it is no longer listened to and the run goes on.
pub struct Control {
    pub(crate) receiver: UnboundedReceiver<Command>,
}

/// The sending end of a control channel, kept by the host (cloneable): an
/// input into a conversation key, pause, resume, cancel.
///
/// Sending never blocks and never fails: a command sent once the run or the
/// session is gone is dropped.
#[derive(Clone)]
pub struct ControlSender {
    sender: UnboundedSender<Command>,
}

/// A new control channel: the sender the host keeps, and the `Control` it
/// hands to the run or the session.
pub fn control() -> (ControlSender, Control) {
    let (sender, receiver) = unbounded();
    (ControlSender { sender }, Control { receiver })
}

impl ControlSender {
    /// Sends a user input into the conversation key `key`.
    pub fn send(&self, key: Key, input: UserInput) {
        let _ = self.sender.unbounded_send(Command::Input { key, input });
    }

    /// Asks the run to pause at the end of its superstep.
    pub fn pause(&self) {
        let _ = self.sender.unbounded_send(Command::Pause);
    }

    /// Resumes a paused session; ignored outside a pause.
    pub fn resume(&self) {
        let _ = self.sender.unbounded_send(Command::Resume);
    }

    /// Stops the superstep at once: the running nodes are dropped.
    pub fn cancel(&self) {
        let _ = self.sender.unbounded_send(Command::Cancel);
    }
}

impl Control {
    pub(crate) fn try_next(&mut self) -> Option<Command> {
        self.receiver.try_recv().ok()
    }
}
