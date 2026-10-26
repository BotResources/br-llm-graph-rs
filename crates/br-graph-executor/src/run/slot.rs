use std::sync::Arc;

use br_graph_contract::{Fields, Input, InputEnd, InstanceId, Node, State};

use super::guard::AbortGuard;

enum Live<F: Fields> {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Presence {
    Absent,
    Creating,
    Running,
}

pub(super) struct NodeSlot<F: Fields> {
    node: Arc<dyn Node<F>>,
    kind: String,
    subkind: String,
    live: Live<F>,
    held: bool,
    untaken: bool,
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

    pub(super) fn node(&self) -> Arc<dyn Node<F>> {
        Arc::clone(&self.node)
    }

    pub(super) fn kind(&self) -> &str {
        &self.kind
    }

    pub(super) fn subkind(&self) -> &str {
        &self.subkind
    }

    pub(super) fn presence(&self) -> Presence {
        match self.live {
            Live::Absent => Presence::Absent,
            Live::Creating { .. } => Presence::Creating,
            Live::Running { .. } => Presence::Running,
        }
    }

    pub(super) fn is_live(&self) -> bool {
        self.presence() != Presence::Absent
    }

    pub(super) fn is_held(&self) -> bool {
        self.held
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

    pub(super) fn first_input_of_creation(&self, id: InstanceId) -> Option<Arc<State<F>>> {
        match &self.live {
            Live::Creating {
                id: creating,
                first_input,
                ..
            } if *creating == id => Some(Arc::clone(first_input)),
            _ => None,
        }
    }

    pub(super) fn hold(&mut self) {
        self.held = true;
    }

    pub(super) fn release(&mut self) {
        self.held = false;
    }

    pub(super) fn leave_untaken(&mut self) {
        self.untaken = true;
    }

    pub(super) fn begin_creating(
        &mut self,
        id: InstanceId,
        first_input: Arc<State<F>>,
        creation: AbortGuard,
    ) {
        self.live = Live::Creating {
            id,
            first_input,
            _creation: creation,
        };
    }

    pub(super) fn become_running(
        &mut self,
        id: InstanceId,
        input: InputEnd<F>,
        forwarder: AbortGuard,
    ) {
        self.live = Live::Running {
            _forwarder: forwarder,
            id,
            input,
        };
    }

    pub(super) fn end_instance(&mut self) {
        self.live = Live::Absent;
    }

    pub(super) fn abandon(&mut self) {
        self.end_instance();
        self.untaken = true;
    }

    pub(super) fn give(&self, state: Arc<State<F>>) -> Option<InstanceId> {
        let Live::Running { id, input, .. } = &self.live else {
            return None;
        };
        input.give(Input::new(state)).ok().map(|_| *id)
    }
}
