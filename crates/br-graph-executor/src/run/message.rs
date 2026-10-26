use br_graph_contract::{Cause, Event, EventEndError, Fields, Instance, InstanceId};

pub(super) struct CreationFailure {
    pub(super) reason: String,
    pub(super) cause: Option<Cause>,
}

pub(super) enum Message<F: Fields> {
    Created {
        node: String,
        id: InstanceId,
        result: Result<Instance<F>, CreationFailure>,
    },
    Ended {
        node: String,
        id: InstanceId,
        result: Result<Event<F>, EventEndError>,
    },
}
