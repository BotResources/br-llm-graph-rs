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

#[tokio::test(start_paused = true)]
async fn an_observer_that_panics_does_not_fail_the_run() {
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
    let outcome = run.await.unwrap().unwrap();
    assert_eq!(value_of(&ended(outcome), "k"), Some(1));
}

#[tokio::test(start_paused = true)]
async fn a_run_gives_the_same_outcome_with_and_without_an_observer() {
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
        finals.push(ended(run.await.unwrap().unwrap()));
    }
    assert_eq!(finals[0], finals[1]);
}

#[tokio::test(start_paused = true)]
async fn a_resume_reports_through_the_same_observer_contract() {
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
    ended(run.await.unwrap().unwrap());
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
async fn the_run_ends_while_the_observer_is_still_held_on_its_first_observation() {
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
