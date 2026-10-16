//! The run: a graph advanced superstep by superstep from its entry or a cursor
//! until it ends, pauses, is cancelled or fails, and the control channel
//! through which the host sends it inputs, pause, resume and cancel.
//!
//! A run returns an `Outcome`, or a `RunFailure` with its checkpoint. Each node
//! of a superstep that finishes records its updates as a pending entry under
//! its occurrence, so a run resumed with a checkpoint's state, cursor and
//! pending entries applies the interrupted superstep once, running only the
//! nodes and map items that did not finish.

mod checkpoint;
mod control;
mod cursor;
mod outcome;
mod pending;
#[path = "loop.rs"]
mod runner;
mod settle;
mod superstep;

#[cfg(test)]
mod cancel_tests;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
pub(crate) mod counted;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod gates;
#[cfg(test)]
mod item_failure_tests;
#[cfg(test)]
mod map_concurrency_tests;
#[cfg(test)]
mod map_tests;
#[cfg(test)]
mod map_typed_tests;
#[cfg(test)]
mod map_window_tests;
#[cfg(test)]
mod nested_map_tests;
#[cfg(test)]
mod origin_tests;
#[cfg(test)]
mod panic_tests;
#[cfg(test)]
mod record_event_tests;
#[cfg(test)]
mod resume_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod superstep_resume_tests;
#[cfg(test)]
pub(crate) mod test_support;

pub use checkpoint::Checkpoint;
pub(crate) use control::Command;
pub use control::{Control, ControlSender, control};
pub use cursor::Cursor;
pub use outcome::{Outcome, RunFailure};
pub use pending::{PendingEntry, PendingWrites};
pub use runner::run;
pub(crate) use runner::run_nested;
