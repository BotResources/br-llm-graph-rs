use br_agentic_contract::{AGENTIC, AgenticNode, CancelView, InstanceId, Node};
use br_graph_contract::{CreateError, CreateFuture};

use crate::common::*;

struct Reader {
    has_port: bool,
}

impl Node<Doc> for Reader {
    fn name(&self) -> &str {
        "reader"
    }

    fn kind(&self) -> &str {
        AGENTIC
    }

    fn subkind(&self) -> &str {
        "react_agent"
    }

    fn create(&self, _cancel: CancelView, _id: InstanceId) -> CreateFuture<'_, Doc> {
        Box::pin(async { Err(CreateError::new("never created")) })
    }
}

impl AgenticNode<Doc> for Reader {
    fn has_incoming(&self) -> bool {
        self.has_port
    }
}

#[test]
fn given_an_agentic_node_then_it_tells_whether_it_has_an_incoming_port_and_its_kind_is_agentic() {
    let with: &dyn AgenticNode<Doc> = &Reader { has_port: true };
    let without: &dyn AgenticNode<Doc> = &Reader { has_port: false };

    assert!(with.has_incoming());
    assert!(!without.has_incoming());
    assert_eq!(with.kind(), AGENTIC);
}
