use std::fmt;
use std::num::NonZeroUsize;

use super::measure::{serialize, serialized_size};
use super::{ApplyError, Fields, InvalidState};

pub const DEFAULT_STATE_LIMIT: NonZeroUsize = match NonZeroUsize::new(131_072) {
    Some(limit) => limit,
    None => NonZeroUsize::MIN,
};

#[derive(Clone, PartialEq)]
pub struct State<F: Fields> {
    fields: F,
    limit: NonZeroUsize,
    size: usize,
}

impl<F: Fields> State<F> {
    pub fn create(fields: F) -> Result<Self, InvalidState> {
        Self::create_with_limit(fields, DEFAULT_STATE_LIMIT)
    }

    pub fn create_with_limit(fields: F, limit: NonZeroUsize) -> Result<Self, InvalidState> {
        fields.validate().map_err(InvalidState::new)?;
        let size = serialized_size(&fields)?;
        if size > limit.get() {
            return Err(InvalidState::oversize(size, limit));
        }
        Ok(Self {
            fields,
            limit,
            size,
        })
    }

    pub fn rebuild(json: &[u8]) -> Result<Self, InvalidState> {
        Self::rebuild_with_limit(json, DEFAULT_STATE_LIMIT)
    }

    pub fn rebuild_with_limit(json: &[u8], limit: NonZeroUsize) -> Result<Self, InvalidState> {
        let fields: F = serde_json::from_slice(json)
            .map_err(|error| InvalidState::new(format!("malformed state: {error}")))?;
        Self::create_with_limit(fields, limit)
    }

    pub fn apply(&self, update: F::Update) -> Result<Self, ApplyError> {
        let mut fields = self.fields.clone();
        fields.apply(update).map_err(ApplyError::Refused)?;
        Self::create_with_limit(fields, self.limit).map_err(ApplyError::InvalidState)
    }

    pub fn fields(&self) -> &F {
        &self.fields
    }

    pub fn limit(&self) -> NonZeroUsize {
        self.limit
    }

    pub fn serialized_size(&self) -> usize {
        self.size
    }

    pub fn to_json(&self) -> Result<Vec<u8>, InvalidState> {
        serialize(&self.fields)
    }

    pub fn into_fields(self) -> F {
        self.fields
    }
}

impl<F: Fields> fmt::Debug for State<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("serialized_size", &self.size)
            .field("limit", &self.limit)
            .finish_non_exhaustive()
    }
}
