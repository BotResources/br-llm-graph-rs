use br_llm_messages::{BlockId, EntryId, Error, TurnId};
use uuid::Uuid;

use crate::fault::Fault;

pub(crate) fn entry_id() -> Result<EntryId, Fault> {
    EntryId::from_uuid(Uuid::now_v7()).map_err(Fault::value)
}

pub(crate) fn turn_id() -> Result<TurnId, Fault> {
    TurnId::from_uuid(Uuid::now_v7()).map_err(Fault::value)
}

pub(crate) fn block_id() -> Result<BlockId, Fault> {
    block_id_raw().map_err(Fault::value)
}

pub(crate) fn block_id_raw() -> Result<BlockId, Error> {
    BlockId::from_uuid(Uuid::now_v7())
}
