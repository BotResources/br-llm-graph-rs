use br_llm_messages::TurnId;

pub trait IdSource: Send + Sync {
    fn turn_id(&self) -> TurnId;
}
