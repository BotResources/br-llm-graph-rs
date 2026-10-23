use std::io;
use std::num::NonZeroUsize;

use serde::Serialize;

use super::InvalidState;

struct Counter {
    bytes: usize,
    limit: usize,
    exceeded: bool,
}

impl io::Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buf.len());
        if self.bytes > self.limit {
            self.exceeded = true;
            return Err(io::Error::other("the size limit is exceeded"));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn serialized_size<T: Serialize>(
    value: &T,
    limit: NonZeroUsize,
) -> Result<usize, InvalidState> {
    let mut counter = Counter {
        bytes: 0,
        limit: limit.get(),
        exceeded: false,
    };
    let written = serde_json::to_writer(&mut counter, value);
    if counter.exceeded {
        return Err(InvalidState::oversize(limit));
    }
    written.map(|()| counter.bytes).map_err(unserializable)
}

pub(super) fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, InvalidState> {
    serde_json::to_vec(value).map_err(unserializable)
}

fn unserializable(error: serde_json::Error) -> InvalidState {
    InvalidState::new(format!("fields cannot be serialized: {error}"))
}
