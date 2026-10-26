use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_graph_executor::{Observation, Observer, OutcomeKind, resume, start};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;

struct Panicking;

impl Observer for Panicking {
    fn observe(&self, _observation: Observation) {
        panic!("the observer panics");
    }
}

struct Poison;

impl Drop for Poison {
    fn drop(&mut self) {
        panic!("the payload dropped");
    }
}

struct PanicsOnce {
    panicked: AtomicBool,
    poisoned: bool,
    recorder: Arc<Recorder>,
}

impl Observer for PanicsOnce {
    fn observe(&self, observation: Observation) {
        if !self.panicked.swap(true, Ordering::SeqCst) {
            if self.poisoned {
                std::panic::panic_any(Poison);
            }
            panic!("the observer panics once");
        }
        self.recorder.observe(observation);
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_observer_that_panics_once_when_the_run_goes_on_then_it_still_hears_the_later_observations()
 {
    hears_the_later_observations(false).await;
}

#[tokio::test(start_paused = true)]
async fn given_an_observer_that_panics_once_with_a_payload_whose_drop_panics_when_the_run_goes_on_then_it_still_hears_the_later_observations()
 {
    hears_the_later_observations(true).await;
}

async fn hears_the_later_observations(poisoned: bool) {
    let rig = chain("1", &["a", "b"]);
    let recorder = Recorder::new();
    let (mut config, ..) = Setup::new().config();
    config.observer = Some(Arc::new(PanicsOnce {
        panicked: AtomicBool::new(false),
        poisoned,
        recorder: Arc::clone(&recorder),
    }));
    let run = tokio::spawn(start(
        rig.graph.clone(),
        state(&[]),
        vec!["a".to_owned()],
        config,
    ));
    rig.stage.detach();
    let mut a = rig.stage.instance("a").await;
    a.next_input().await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    b.next_input().await;
    b.result(Change::Empty);
    assert_eq!(
        value_of(&ended(within(run).await.unwrap().unwrap()), "k"),
        Some(1)
    );
    let log = within(recorder.wait_for_outcome()).await;
    assert!(log.iter().any(created("b")));
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunEnded))
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_observer_that_always_panics_when_the_run_goes_on_then_the_run_is_not_failed() {
    let rig = chain("1", &["a", "b"]);
    let (mut config, ..) = Setup::new().config();
    config.observer = Some(Arc::new(Panicking));
    let run = tokio::spawn(start(
        rig.graph.clone(),
        state(&[]),
        vec!["a".to_owned()],
        config,
    ));
    rig.stage.detach();
    let mut a = rig.stage.instance("a").await;
    a.next_input().await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    b.next_input().await;
    b.result(Change::Empty);
    let outcome = within(run).await.unwrap().unwrap();
    assert_eq!(value_of(&ended(outcome), "k"), Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_run_when_it_has_no_observer_then_the_outcome_is_the_same() {
    let mut finals = Vec::new();
    for with_observer in [true, false] {
        let rig = chain("1", &["a", "b"]);
        let (mut config, ..) = Setup::new().config();
        if !with_observer {
            config.observer = None;
        }
        let run = tokio::spawn(start(
            rig.graph.clone(),
            state(&[]),
            vec!["a".to_owned()],
            config,
        ));
        let mut a = rig.stage.instance("a").await;
        a.next_input().await;
        a.result(set("k", 1));
        let mut b = rig.stage.instance("b").await;
        b.next_input().await;
        b.result(set("j", 2));
        finals.push(ended(within(run).await.unwrap().unwrap()));
    }
    assert_eq!(finals[0], finals[1]);
}

#[tokio::test(start_paused = true)]
async fn given_a_resumed_run_when_observed_then_the_same_contract_holds() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    let (config, _, observer, _) = Setup::new().config();
    rig.stage.attach(observer.clone());
    let run = tokio::spawn(resume(rig.graph.clone(), checkpoint, BIG, config));
    let mut again = rig.stage.instance("a").await;
    again.result(Change::Empty);
    ended(within(run).await.unwrap().unwrap());
    let log = observer.wait_for_outcome().await;
    assert!(position(&log, created("a")) < position(&log, handed("a")));
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunEnded))
    );
}

struct HeldUntilReleased {
    release: Mutex<Receiver<()>>,
}

impl Observer for HeldUntilReleased {
    fn observe(&self, _observation: Observation) {
        tokio::task::block_in_place(|| {
            let _ = self.release.lock().unwrap().recv();
        });
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn given_an_observer_held_on_its_first_observation_when_the_run_ends_then_the_run_does_not_wait_for_it()
 {
    let rig = single("a");
    let (release, receiver) = channel();
    let (mut config, ..) = Setup::new().config();
    config.observer = Some(Arc::new(HeldUntilReleased {
        release: Mutex::new(receiver),
    }));
    let run = tokio::spawn(start(
        rig.graph.clone(),
        state(&[]),
        vec!["a".to_owned()],
        config,
    ));
    let mut a = rig.stage.instance("a").await;
    a.next_input().await;
    a.result(set("k", 1));
    let outcome = tokio::time::timeout(Duration::from_secs(30), run)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(value_of(&ended(outcome), "k"), Some(1));
    for _ in 0..16 {
        let _ = release.send(());
    }
}
