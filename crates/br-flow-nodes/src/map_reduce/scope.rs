use br_graph_contract::{CancelToken, CancelView};

pub(super) struct ChildScope {
    token: CancelToken,
}

impl ChildScope {
    pub(super) fn new(parent: &CancelView) -> Self {
        Self {
            token: parent.child(),
        }
    }

    pub(super) fn view(&self) -> CancelView {
        self.token.view()
    }

    pub(super) fn trigger(&self) {
        self.token.handle().trigger();
    }

    pub(super) fn is_triggered(&self) -> bool {
        self.token.view().is_triggered()
    }
}

impl Drop for ChildScope {
    fn drop(&mut self) {
        self.trigger();
    }
}
