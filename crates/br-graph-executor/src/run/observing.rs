use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::panic_text::release;
use crate::{Observation, Observer};

pub(super) struct Observing {
    channel: Option<UnboundedSender<Observation>>,
}

impl Observing {
    pub(super) fn spawn(observer: Option<Arc<dyn Observer>>) -> Self {
        let channel = observer.map(|observer| {
            let (sender, mut receiver) = unbounded_channel::<Observation>();
            tokio::spawn(async move {
                while let Some(observation) = receiver.recv().await {
                    if let Err(payload) =
                        catch_unwind(AssertUnwindSafe(|| observer.observe(observation)))
                    {
                        release(payload);
                    }
                }
            });
            sender
        });
        Self { channel }
    }

    pub(super) fn tell(&self, make: impl FnOnce() -> Observation) {
        if let Some(channel) = &self.channel {
            let _ = channel.send(make());
        }
    }
}
