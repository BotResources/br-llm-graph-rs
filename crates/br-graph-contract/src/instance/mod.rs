mod core;
mod dropped;
mod error;
mod event;
mod event_end;
mod input;
mod input_end;
mod lifecycle;
mod reader;
#[cfg(test)]
mod reentrancy_tests;
mod sender;
#[cfg(test)]
mod tests;

pub use dropped::Dropped;
pub use error::{EventEndError, InputClosed};
pub use event::Event;
pub use event_end::EventEnd;
pub use input::Input;
pub use input_end::InputEnd;
pub use reader::InputReader;
pub use sender::EventSender;

use std::fmt;
use std::sync::Arc;

use crate::{Fields, InstanceId};

pub struct Instance<F: Fields> {
    id: InstanceId,
    input: InputEnd<F>,
    events: EventEnd<F>,
}

impl<F: Fields> Instance<F> {
    pub fn channel(id: InstanceId) -> (Instance<F>, InputReader<F>, EventSender<F>) {
        let core = Arc::new(core::Core::new(id));
        let instance = Instance {
            id,
            input: InputEnd::new(Arc::clone(&core)),
            events: EventEnd::new(Arc::clone(&core)),
        };
        (
            instance,
            InputReader::new(Arc::clone(&core)),
            EventSender::new(core),
        )
    }

    pub fn id(&self) -> InstanceId {
        self.id
    }

    pub fn into_parts(self) -> (InputEnd<F>, EventEnd<F>) {
        (self.input, self.events)
    }
}

impl<F: Fields> fmt::Debug for Instance<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance").field("id", &self.id).finish()
    }
}
