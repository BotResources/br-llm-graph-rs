use std::sync::Arc;

use br_graph_contract::{Fields, InputEnd, InstanceId, Node, State};

use super::guard::AbortGuard;

pub(super) enum Live<F: Fields> {
    Absent,
    Creating {
        id: InstanceId,
        first_input: Arc<State<F>>,
        _creation: AbortGuard,
    },
    Running {
        _forwarder: AbortGuard,
        id: InstanceId,
        input: InputEnd<F>,
    },
}

pub(super) struct NodeSlot<F: Fields> {
    pub(super) node: Arc<dyn Node<F>>,
    pub(super) kind: String,
    pub(super) subkind: String,
    pub(super) live: Live<F>,
    pub(super) held: bool,
    pub(super) untaken: bool,
}

impl<F: Fields> NodeSlot<F> {
    pub(super) fn new(node: Arc<dyn Node<F>>, kind: String, subkind: String) -> Self {
        Self {
            node,
            kind,
            subkind,
            live: Live::Absent,
            held: false,
            untaken: false,
        }
    }

    pub(super) fn is_live(&self) -> bool {
        !matches!(self.live, Live::Absent)
    }

    pub(super) fn is_idle(&self) -> bool {
        !self.is_live() && !self.held && !self.untaken
    }

    pub(super) fn running_id(&self) -> Option<InstanceId> {
        match &self.live {
            Live::Running { id, .. } => Some(*id),
            _ => None,
        }
    }

    pub(super) fn abandon(&mut self) {
        self.live = Live::Absent;
        self.untaken = true;
    }
}
