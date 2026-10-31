use std::future::Future;
use std::sync::Arc;

use br_agentic_contract::{InboxContext, cancelled, failed, result};
use br_graph_contract::{CancelView, Event, EventSender, Fields, InputReader, InstanceId};
use br_llm_messages::{AgentId, ClosureReason};

use crate::config::Common;
use crate::fault::{Fault, Stop};
use crate::seats::{SeatSpec, Seats};
use crate::signals::{Raced, Signals, guarded};
use crate::start::agent_label;
use crate::worker::{Worker, WorkerParts};

pub(crate) trait Body<F: Fields>: Send + Sync + 'static {
    fn common(&self) -> &Common<F>;

    fn seats(&self) -> Vec<SeatSpec<'_, F>>;

    fn execute<'w>(
        &'w self,
        worker: &'w mut Worker<'_>,
        seats: &'w Seats,
        snapshot: &'w F,
    ) -> impl Future<Output = Result<F::Update, Stop>> + Send + 'w;
}

pub(crate) fn settle<F: Fields>(
    reader: &mut InputReader<F>,
    sender: EventSender<F>,
    event: Event<F>,
) {
    reader.close();
    drop(reader.drain());
    sender.finish(event);
}

async fn identify<F: Fields, B: Body<F>>(
    body: &B,
    snapshot: &F,
    instance: InstanceId,
) -> Result<
    (
        br_agentic_contract::ThreadRef,
        Vec<(br_agentic_contract::AgentLabel, AgentId)>,
    ),
    Stop,
> {
    let common = body.common();
    let thread = common.ports.thread.thread(snapshot).await?;
    let mut agents = Vec::new();
    for spec in body.seats() {
        let label = agent_label(spec.label)?;
        let agent = common.ports.agent.agent(snapshot, instance, &label).await?;
        agents.push((label, agent));
    }
    Ok((thread, agents))
}

fn bare_end<F: Fields>(raced: Raced<Result<Worker<'_>, Stop>>) -> Option<Event<F>> {
    match raced {
        Raced::Gone | Raced::Done(Ok(_)) => None,
        Raced::Cancelled | Raced::Done(Err(Stop::Cancelled)) => Some(Event::Cancelled),
        Raced::Done(Err(Stop::Fault(fault))) => Some(Event::failed(fault)),
    }
}

pub(crate) async fn run<F: Fields, B: Body<F>>(
    body: Arc<B>,
    mut reader: InputReader<F>,
    sender: EventSender<F>,
    cancel: CancelView,
) {
    let mut signals = Signals::new(&cancel, &sender);
    let input = match signals.next_input(&mut reader).await {
        Raced::Gone => return,
        Raced::Cancelled => return settle(&mut reader, sender, Event::Cancelled),
        Raced::Done(input) => input,
    };
    let instance = reader.id();
    let snapshot = input.fields();
    let common = body.common();
    let (thread, agents) = match signals
        .race(guarded(identify(&*body, snapshot, instance)))
        .await
    {
        Raced::Gone => return,
        Raced::Cancelled | Raced::Done(Err(Stop::Cancelled)) => {
            return settle(&mut reader, sender, Event::Cancelled);
        }
        Raced::Done(Err(Stop::Fault(fault))) => {
            return settle(&mut reader, sender, Event::failed(fault));
        }
        Raced::Done(Ok(identified)) => identified,
    };
    let Some(main) = agents.first().map(|(_, agent)| *agent) else {
        return settle(
            &mut reader,
            sender,
            Event::failed(Fault::Value("the node has no agent".to_owned())),
        );
    };
    let context = InboxContext {
        instance,
        thread,
        agent: main,
    };
    let parts = WorkerParts {
        access: &*common.ports.access,
        inbox: common.ports.inbox.clone(),
        context,
        step_limit: common.settings.step_limit,
        timeouts: common.settings.timeouts,
        cancel: cancel.clone(),
    };
    let opened = signals
        .race(guarded(async { Ok(Worker::open(parts).await?) }))
        .await;
    let mut worker = match opened {
        Raced::Done(Ok(worker)) => worker,
        other => {
            if let Some(event) = bare_end(other) {
                settle(&mut reader, sender, event);
            }
            return;
        }
    };
    let specs = body.seats();
    let raced = signals
        .race(guarded(async {
            let seats = worker
                .begin(common, snapshot, instance, &specs, agents)
                .await?;
            body.execute(&mut worker, &seats, snapshot).await
        }))
        .await;
    conclude(&mut worker, &mut reader, sender, raced).await;
}

async fn conclude<F: Fields>(
    worker: &mut Worker<'_>,
    reader: &mut InputReader<F>,
    sender: EventSender<F>,
    raced: Raced<Result<F::Update, Stop>>,
) {
    worker.stop_read();
    let event = match raced {
        Raced::Gone => return,
        Raced::Cancelled | Raced::Done(Err(Stop::Cancelled)) => {
            let _ = worker.session.discard_all().await;
            let _ = worker
                .close_open_turns(&ClosureReason::cancelled(), false)
                .await;
            cancelled(reader, &mut worker.session).await
        }
        Raced::Done(Err(Stop::Fault(fault))) => {
            let _ = worker.session.discard_all().await;
            let reason = ClosureReason::failed(fault.detail())
                .or_else(|_| ClosureReason::failed("the instance failed"));
            if let Ok(reason) = reason {
                let _ = worker.close_open_turns(&reason, false).await;
            }
            failed(reader, &mut worker.session, fault).await
        }
        Raced::Done(Ok(update)) => result(reader, &mut worker.session, update).await,
    };
    settle(reader, sender, event);
}
