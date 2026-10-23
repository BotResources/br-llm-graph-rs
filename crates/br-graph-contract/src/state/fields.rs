use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Reason;

pub trait Fields: Serialize + DeserializeOwned + Clone + PartialEq + Send + Sync + 'static {
    type Update: Send + Sync + 'static;

    fn empty_update() -> Self::Update;

    fn validate(&self) -> Result<(), Reason>;

    fn apply(&mut self, update: Self::Update) -> Result<(), Reason>;
}
