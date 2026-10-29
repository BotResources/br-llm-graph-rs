use std::future::Future;
use std::sync::Arc;

use br_agentic_contract::{AgentLabel, InboxContext, Queue, Session, ThreadRef, ToolSet};
use br_graph_contract::{CancelView, Event, EventSender, Fields, InputReader, InstanceId};
use br_llm_messages::{AgentId, Author, ClosureReason};

use crate::config::Common;
use crate::fault::{Fault, Stop};
use crate::limits::Steps;
use crate::signals::{Raced, Signals, guarded};
use crate::start::{agent_label, author_of};
use crate::worker::Worker;

pub(crate) trait Body<F: Fields>: Send + Sync + 'static {
    fn common(&self) -> &Common<F>;

    fn first_label(&self) -> &str;

    fn first_author(&self) -> Option<&Author>;

    fn execute<'w>(
        &'w self,
        worker: &'w mut Worker<'_>,
        snapshot: &'w F,
        instance: InstanceId,
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

struct Booted {
    thread: ThreadRef,
    label: AgentLabel,
    agent: AgentId,
    author: Author,
    tools: ToolSet,
}

async fn boot<F: Fields, B: Body<F>>(
    body: &B,
    snapshot: &F,
    instance: InstanceId,
) -> Result<Booted, Stop> {
    let common = body.common();
    let label = agent_label(body.first_label())?;
    let thread = common.ports.thread.thread(snapshot).await?;
    let agent = common.ports.agent.agent(snapshot, instance, &label).await?;
    let author = author_of(body.first_author(), &agent, &label)?;
    Ok(Booted {
        thread,
        label,
        agent: agent.id,
        author,
        tools: ToolSet::new(Vec::new(), Vec::new()).map_err(Fault::from)?,
    })
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
    let booted = match signals
        .race(guarded(boot(&*body, snapshot, instance)))
        .await
    {
        Raced::Gone => return,
        Raced::Cancelled | Raced::Done(Err(Stop::Cancelled)) => {
            return settle(&mut reader, sender, Event::Cancelled);
        }
        Raced::Done(Err(Stop::Fault(fault))) => {
            return settle(&mut reader, sender, Event::failed(fault));
        }
        Raced::Done(Ok(booted)) => booted,
    };
    let Booted {
        thread,
        label,
        agent,
        author,
        tools,
    } = booted;
    let context = InboxContext {
        instance,
        thread,
        agent,
    };
    let access = &*common.ports.access;
    let mut worker = Worker {
        access,
        inbox: common.ports.inbox.as_deref(),
        context,
        session: Session::new(access, thread, agent),
        queue: Queue::new(),
        author,
        system: None,
        tools,
        files: None,
        steps: Steps::new(common.settings.step_limit),
        timeouts: common.settings.timeouts,
        cancel: cancel.clone(),
    };
    let raced = signals
        .race(guarded(async {
            worker.begin(common, snapshot, instance, &label).await?;
            body.execute(&mut worker, snapshot, instance).await
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
    let _ = worker.session.discard_all().await;
    match raced {
        Raced::Gone => {}
        Raced::Cancelled | Raced::Done(Err(Stop::Cancelled)) => {
            worker.close_open_quietly(ClosureReason::cancelled()).await;
            settle(reader, sender, Event::Cancelled);
        }
        Raced::Done(Err(Stop::Fault(fault))) => {
            let reason = ClosureReason::failed(fault.detail())
                .or_else(|_| ClosureReason::failed("the instance failed"));
            if let Ok(reason) = reason {
                worker.close_open_quietly(reason).await;
            }
            settle(reader, sender, Event::failed(fault));
        }
        Raced::Done(Ok(update)) => settle(reader, sender, Event::Result(update)),
    }
}
