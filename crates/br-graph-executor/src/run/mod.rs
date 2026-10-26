mod checkpointing;
mod conclude;
mod created;
mod ended;
mod guard;
mod handover;
mod instances;
mod message;
mod observing;
mod origin;
mod result_step;
mod slot;
mod stopping;

use std::collections::BTreeMap;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{CancelToken, Fields, Graph, State, Structure};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::{Instant, sleep_until};

use crate::{CheckpointSink, Outcome, RunConfig, RunFailure};
use message::Message;
use observing::Observing;
use slot::NodeSlot;

pub(crate) use origin::Origin;

#[derive(Clone, Copy)]
enum Trigger {
    Host,
    Executor,
}

enum Phase {
    Running,
    Stopping { deadline: Instant, trigger: Trigger },
}

enum Wake<F: Fields> {
    CancelRequested,
    Message(Message<F>),
    Deadline,
}

pub(crate) struct Run<F: Fields> {
    graph: Arc<Graph<F>>,
    structure: Structure,
    cancel: CancelToken,
    grace: Duration,
    sink: Option<Arc<dyn CheckpointSink>>,
    observing: Observing,
    state: Arc<State<F>>,
    slots: BTreeMap<String, NodeSlot<F>>,
    failures: Vec<RunFailure>,
    phase: Phase,
    sender: UnboundedSender<Message<F>>,
    receiver: UnboundedReceiver<Message<F>>,
}

impl<F: Fields> Run<F> {
    pub(crate) fn new(graph: Arc<Graph<F>>, state: Arc<State<F>>, config: RunConfig) -> Self {
        let structure = graph.structure();
        let slots: BTreeMap<String, NodeSlot<F>> = structure
            .nodes()
            .iter()
            .filter_map(|described| {
                let node = graph.node(&described.name)?;
                let slot = NodeSlot::new(
                    Arc::clone(node),
                    described.kind.clone(),
                    described.subkind.clone(),
                );
                Some((described.name.clone(), slot))
            })
            .collect();
        let (sender, receiver) = unbounded_channel();
        Self {
            graph,
            structure,
            cancel: config.cancel,
            grace: config.grace,
            sink: config.sink,
            observing: Observing::spawn(config.observer),
            state,
            slots,
            failures: Vec::new(),
            phase: Phase::Running,
            sender,
            receiver,
        }
    }

    pub(crate) async fn drive(mut self, origin: Origin) -> Outcome<F> {
        for name in origin.held_names() {
            if let Some(slot) = self.slots.get_mut(name) {
                slot.hold();
            }
        }
        self.read_token();
        if self.is_stopping() {
            return self.conclude(origin.into_checkpoint());
        }
        self.hand_over(origin.first_targets());
        let mut cancel_requested = pin!(self.cancel.view().triggered());
        loop {
            self.read_token();
            if self.all_idle() {
                return self.conclude(None);
            }
            let wake = match self.phase {
                Phase::Running => tokio::select! {
                    biased;
                    () = &mut cancel_requested => Wake::CancelRequested,
                    Some(message) = self.receiver.recv() => Wake::Message(message),
                },
                Phase::Stopping { deadline, .. } => {
                    if !self.any_live() || self.grace.is_zero() {
                        self.abandon_all_live();
                        return self.conclude(None);
                    }
                    tokio::select! {
                        biased;
                        Some(message) = self.receiver.recv() => Wake::Message(message),
                        () = sleep_until(deadline) => Wake::Deadline,
                    }
                }
            };
            match wake {
                Wake::CancelRequested => self.read_token(),
                Wake::Message(message) => self.step(message).await,
                Wake::Deadline => {
                    self.abandon_all_live();
                    return self.conclude(None);
                }
            }
        }
    }

    async fn step(&mut self, message: Message<F>) {
        self.read_token();
        match message {
            Message::Created { node, id, result } => self.on_created(node, id, result),
            Message::Ended { node, id, result } => self.on_ended(node, id, result).await,
        }
    }

    fn all_idle(&self) -> bool {
        self.slots.values().all(NodeSlot::is_idle)
    }

    fn any_live(&self) -> bool {
        self.slots.values().any(NodeSlot::is_live)
    }
}
