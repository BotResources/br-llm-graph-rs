use br_agentic_contract::MailId;
use br_graph_contract::InstanceId;
use br_llm_messages::{
    AgentId, Author, BlockId, EntryId, HumanId, PlainBlock, TextBlock, TurnId, Uuid,
};

pub fn uuid() -> Uuid {
    Uuid::now_v7()
}

pub fn entry_id() -> EntryId {
    EntryId::from_uuid(uuid()).unwrap()
}

pub fn turn_id() -> TurnId {
    TurnId::from_uuid(uuid()).unwrap()
}

pub fn block_id() -> BlockId {
    BlockId::from_uuid(uuid()).unwrap()
}

pub fn agent_id() -> AgentId {
    AgentId::from_uuid(uuid()).unwrap()
}

pub fn human_id() -> HumanId {
    HumanId::from_uuid(uuid()).unwrap()
}

pub fn mail_id() -> MailId {
    MailId::new(entry_id())
}

pub fn instance_id() -> InstanceId {
    InstanceId::new(uuid()).unwrap()
}

pub fn author(name: &str) -> Author {
    Author::new(name).unwrap()
}

pub fn text_block(text: &str) -> PlainBlock {
    PlainBlock::Text(TextBlock::new(block_id(), text).unwrap())
}
