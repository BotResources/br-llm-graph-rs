//! The tests' wait at a gate beside a run: a test that holds calls at a
//! gate fails on the run's outcome when the run ends first, rather than
//! hanging.

/// Awaits `run` beside `check`, a check that waits at a gate for the calls
/// the run holds there. When the run ends first, the check would wait
/// forever: the run's result is given back instead (`Err`), for the test to
/// fail on it rather than hang.
pub(crate) async fn beside<R, C>(
    run: impl std::future::Future<Output = R>,
    check: impl std::future::Future<Output = C>,
) -> Result<(R, C), R> {
    futures_util::pin_mut!(run);
    futures_util::pin_mut!(check);
    match futures_util::future::select(run, check).await {
        futures_util::future::Either::Left((ended, _check)) => Err(ended),
        futures_util::future::Either::Right((checked, run)) => Ok((run.await, checked)),
    }
}
