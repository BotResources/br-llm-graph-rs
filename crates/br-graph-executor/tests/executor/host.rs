use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{CancelHandle, CancelToken, State};
use br_graph_executor::{Checkpoint, Outcome, RunConfig, RunFailure, resume, start};
use tokio::task::JoinHandle;

use crate::graphs::Rig;
use crate::kv::Kv;
use crate::observer::Recorder;
use crate::sink::RecordingSink;

pub const GRACE: Duration = Duration::from_secs(10);
pub const BIG: NonZeroUsize = NonZeroUsize::new(1 << 20).unwrap();

pub struct Host {
    pub cancel: CancelHandle,
    pub observer: Arc<Recorder>,
    pub sink: Option<Arc<RecordingSink>>,
    run: JoinHandle<Outcome<Kv>>,
}

pub struct Setup {
    pub grace: Duration,
    pub with_sink: bool,
    pub token: Option<CancelToken>,
}

impl Setup {
    pub fn new() -> Self {
        Self {
            grace: GRACE,
            with_sink: false,
            token: None,
        }
    }

    pub fn grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    pub fn sink(mut self) -> Self {
        self.with_sink = true;
        self
    }

    pub fn token(mut self, token: CancelToken) -> Self {
        self.token = Some(token);
        self
    }

    pub fn config(
        self,
    ) -> (
        RunConfig,
        CancelHandle,
        Arc<Recorder>,
        Option<Arc<RecordingSink>>,
    ) {
        let token = self.token.unwrap_or_default();
        let cancel = token.handle().clone();
        let observer = Recorder::new();
        let sink = self.with_sink.then(RecordingSink::new);
        let mut config = RunConfig::new(token, self.grace);
        config.observer = Some(Arc::clone(&observer) as _);
        config.sink = sink.clone().map(|sink| sink as _);
        (config, cancel, observer, sink)
    }
}

pub fn begin(rig: &Rig, state: State<Kv>, nodes: &[&str], setup: Setup) -> Host {
    let (config, cancel, observer, sink) = setup.config();
    let nodes: Vec<String> = nodes.iter().map(|name| (*name).to_owned()).collect();
    rig.stage.attach(Arc::clone(&observer));
    let graph = Arc::clone(&rig.graph);
    let run = tokio::spawn(async move {
        match start(graph, state, nodes, config).await {
            Ok(outcome) => outcome,
            Err(error) => panic!("start refused: {error}"),
        }
    });
    Host {
        cancel,
        observer,
        sink,
        run,
    }
}

pub fn begin_resume(rig: &Rig, checkpoint: Checkpoint, limit: NonZeroUsize, setup: Setup) -> Host {
    let (config, cancel, observer, sink) = setup.config();
    rig.stage.attach(Arc::clone(&observer));
    let graph = Arc::clone(&rig.graph);
    let run = tokio::spawn(async move {
        match resume(graph, checkpoint, limit, config).await {
            Ok(outcome) => outcome,
            Err(error) => panic!("resume refused: {error}"),
        }
    });
    Host {
        cancel,
        observer,
        sink,
        run,
    }
}

pub const HANG_LIMIT: Duration = Duration::from_secs(7 * 24 * 3600);

pub async fn within<T>(waiting: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(HANG_LIMIT, waiting).await {
        Ok(value) => value,
        Err(_) => panic!("nothing happened within a week of virtual time: the test would hang"),
    }
}

impl Host {
    pub async fn outcome(self) -> Outcome<Kv> {
        within(self.run).await.unwrap()
    }

    pub fn drop_the_run(&self) {
        self.run.abort();
    }

    pub fn is_finished(&self) -> bool {
        self.run.is_finished()
    }
}

pub fn alive_tasks() -> usize {
    tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks()
}

pub async fn tasks_back_to(baseline: usize) -> usize {
    for _ in 0..1_000 {
        if alive_tasks() <= baseline {
            break;
        }
        tokio::task::yield_now().await;
    }
    alive_tasks()
}

pub async fn quiesce() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

pub fn handwritten_checkpoint(rig: &Rig, state_json: &str, nodes: &[&str]) -> Checkpoint {
    let structure = serde_json::to_string(&rig.graph.structure()).unwrap();
    let names = serde_json::to_string(nodes).unwrap();
    let document = format!(r#"{{"state":{state_json},"structure":{structure},"nodes":{names}}}"#);
    Checkpoint::from_json_slice(document.as_bytes()).unwrap()
}

pub fn state_of(checkpoint: &Checkpoint) -> Kv {
    State::<Kv>::rebuild_with_limit(checkpoint.state_json().as_bytes(), BIG)
        .unwrap()
        .into_fields()
}

pub fn failed(outcome: Outcome<Kv>) -> (Vec<RunFailure>, Checkpoint) {
    match outcome {
        Outcome::RunFailed {
            failures,
            checkpoint,
        } => (failures, checkpoint),
        other => panic!("expected RunFailed, got {other:?}"),
    }
}

pub fn cancelled(outcome: Outcome<Kv>) -> (Vec<RunFailure>, Checkpoint) {
    match outcome {
        Outcome::RunCancelled {
            failures,
            checkpoint,
        } => (failures, checkpoint),
        other => panic!("expected RunCancelled, got {other:?}"),
    }
}

pub fn ended(outcome: Outcome<Kv>) -> Kv {
    match outcome {
        Outcome::RunEnded { state } => state.into_fields(),
        other => panic!("expected RunEnded, got {other:?}"),
    }
}
