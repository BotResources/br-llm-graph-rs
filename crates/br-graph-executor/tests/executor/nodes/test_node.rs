use std::sync::Arc;

use br_graph_contract::{CancelView, CreateError, CreateFuture, Instance, InstanceId, Node};
use uuid::Uuid;

use crate::kv::Kv;

use super::gates::{DropFlag, DropGuard, Gate};
use super::handle::Handle;
use super::stage::Stage;

pub enum Creation {
    Gated(Gate),
    Fail(&'static str),
    PanicText(&'static str),
    PanicString(String),
    PanicOther,
    WrongId,
    InputClosedAtBirth,
    Never(DropFlag),
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TestError(pub String);

pub struct TestNode {
    pub name: String,
    pub kind: String,
    pub subkind: String,
    pub stage: Arc<Stage>,
}

fn other_id(id: InstanceId) -> InstanceId {
    let bytes = id.as_uuid().as_u128() ^ 1;
    InstanceId::new(Uuid::from_u128(bytes)).unwrap()
}

impl Node<Kv> for TestNode {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn subkind(&self) -> &str {
        &self.subkind
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Kv> {
        let creation = self.stage.next_creation(&self.name);
        self.stage
            .calls
            .lock()
            .unwrap()
            .push((self.name.clone(), id));
        let name = self.name.clone();
        let announce = self.stage.announce.clone();
        Box::pin(async move {
            let mut channel_id = id;
            let mut closed_at_birth = false;
            match creation {
                Some(Creation::Gated(gate)) => gate.passed().await,
                Some(Creation::Fail(message)) => {
                    return Err(CreateError::new(TestError(message.to_owned())));
                }
                Some(Creation::PanicText(message)) => panic!("{message}"),
                Some(Creation::PanicString(message)) => panic!("{}", message),
                Some(Creation::PanicOther) => std::panic::panic_any(42_u32),
                Some(Creation::WrongId) => channel_id = other_id(id),
                Some(Creation::InputClosedAtBirth) => closed_at_birth = true,
                Some(Creation::Never(flag)) => {
                    let _guard = DropGuard(flag);
                    std::future::pending::<()>().await;
                }
                None => {}
            }
            let (instance, reader, sender) = Instance::<Kv>::channel(channel_id);
            if closed_at_birth {
                sender.close_input();
            }
            let dropped = sender.dropped();
            let handle = Handle::new(name, channel_id, cancel, reader, sender, dropped);
            let _ = announce.send(handle);
            Ok(instance)
        })
    }
}
