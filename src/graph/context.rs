use std::sync::Arc;

use br_llm_messages::TurnId;

use crate::observe::Observer;

pub trait IdSource: Send + Sync {
    fn turn_id(&self) -> TurnId;
}

#[derive(Clone)]
pub struct Context {
    pub observer: Arc<dyn Observer>,
    pub ids: Arc<dyn IdSource>,
}

impl Context {
    pub fn new(observer: Arc<dyn Observer>, ids: Arc<dyn IdSource>) -> Self {
        Self { observer, ids }
    }
}
