use std::io;
use std::num::NonZeroUsize;

use serde::Serialize;

use super::InvalidState;

struct Counter {
    bytes: usize,
}

impl io::Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buf.len());
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
    let mut counter = Counter { bytes: 0 };
    serde_json::to_writer(&mut counter, value).map_err(unserializable)?;
    if counter.bytes > limit.get() {
        return Err(InvalidState::oversize(counter.bytes, limit));
    }
    Ok(counter.bytes)
}

pub(super) fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, InvalidState> {
    serde_json::to_vec(value).map_err(unserializable)
}

fn unserializable(error: serde_json::Error) -> InvalidState {
    InvalidState::new(format!("fields cannot be serialized: {error}"))
}
