//! The runner call: a node of a graph (`RunnerCall`) or a tool of an agent
//! (`RunnerTool`) that asks another runner for an answer through the runner
//! port, the host's (`RunnerPort`), and waits for it.
//!
//! The library owns the node, the tool and the call key (`CallKey`: the
//! origin of the node that makes the call and its arguments, and, for a
//! tool, the id of the tool call), so a resumed run, which keeps its run id,
//! gives the keys it gave before and the port finds the calls it had
//! started. Everything else
//! is the port's: the child's thread and job, the wait and its deadline,
//! reading the answer back, finishing or cancelling the job.
//!
//! A call is built from its target (`RunnerTarget`, a runner type that is
//! not blank), its arguments (`RunnerArguments`, a JSON object, fixed or a
//! function of the state and the configuration; for a tool, those of the
//! tool call) and the port. The port gives the child's answer
//! (`RunnerAnswer`: the blocks of its reply in order, `AnswerBlock`, and its
//! thread), its expected failure with a reason, or an error of its own
//! (`RunnerReply`, `RunnerError`).

mod answer;
mod arguments;
mod call;
mod key;
mod port;
mod target;
mod tool;

#[cfg(test)]
mod runner_call_check_tests;
#[cfg(test)]
mod runner_call_loop_tests;
#[cfg(test)]
mod runner_call_map_tests;
#[cfg(test)]
mod runner_call_resume_tests;
#[cfg(test)]
mod runner_call_subgraph_tests;
#[cfg(test)]
mod runner_call_tests;
#[cfg(test)]
mod runner_describe_tests;
#[cfg(test)]
mod runner_tool_agent_tests;
#[cfg(test)]
mod runner_tool_tests;
#[cfg(test)]
mod test_support;

pub use answer::{AnswerBlock, RunnerAnswer};
pub use arguments::RunnerArguments;
pub use call::RunnerCall;
pub use key::CallKey;
pub(crate) use port::RUNNER_CALL_FAILED;
pub use port::{RunnerError, RunnerFuture, RunnerPort, RunnerReply, RunnerRequest};
pub use target::RunnerTarget;
pub use tool::RunnerTool;
