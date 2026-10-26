use br_graph_contract::{InstanceId, Target};

pub trait Observer: Send + Sync {
    fn observe(&self, observation: Observation);
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Observation {
    InstanceCreated {
        instance: InstanceId,
        node: String,
        kind: String,
        subkind: String,
    },
    InputHandedOver {
        node: String,
        instance: InstanceId,
    },
    EventHandled {
        node: String,
        instance: InstanceId,
        event: ObservedEvent,
    },
    UpdateApplied {
        node: String,
    },
    EdgeEvaluated {
        node: String,
        targets: Vec<Target>,
    },
    Outcome(OutcomeKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedEvent {
    Result,
    Failed,
    Refused,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeKind {
    RunEnded,
    RunFailed,
    RunCancelled,
}
