//! The id source: where every new thread id and turn id comes from, and the
//! request that names each one.

use br_llm_messages::{InputId, TurnId};

use crate::origin::Origin;
use crate::value::ThreadId;

/// What an id is asked for: the agent asking, the user input its call
/// answers and the rank of the id within that call.
///
/// - `origin`: the run id and the occurrence of the agent, the node that
///   calls the agent's graph (empty at top level). Every node of one call of
///   an agent asks under the same origin, so the ids its entry looks for are
///   those its model call was given.
/// - `trigger`: the id of the user input the agent answers, the last user
///   input of its `opening`; `None` for an agent in memory whose `opening`
///   holds no user input.
/// - `rank`: 0 for the thread, then 1 for the first turn of the call, 2 for
///   the second...
///
/// The same request gives the same id: a resumed run, which keeps its run
/// id, receives the ids it had. A new trigger, a new run or another
/// occurrence receives new ones.
///
/// Its fields are read by the host's id source and built with `new`: it may
/// gain fields in a later version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct IdRequest {
    /// The run id and the occurrence of the asking agent.
    pub origin: Origin,
    /// The id of the user input the agent's call answers.
    pub trigger: Option<InputId>,
    /// 0 for the thread, then the rank of the turn within the call.
    pub rank: usize,
}

impl IdRequest {
    /// The request of the id of rank `rank` in the call of the agent at
    /// `origin` answering `trigger`.
    pub fn new(origin: Origin, trigger: Option<InputId>, rank: usize) -> Self {
        Self {
            origin,
            trigger,
            rank,
        }
    }
}

/// The host's source of new ids: every thread id and turn id the graph
/// writes comes from it.
///
/// It must give the same id for the same request, and a new one for any
/// other request, so a resumed run finds what it wrote and a new call never
/// meets the ids of another. How it makes them (a hash of the request, a
/// table it keeps...) is the host's.
///
/// Two runs at once of agents that write ids need distinct run ids, for
/// their ids only: under one run id, the same occurrence and the same
/// trigger, they would ask the same requests and write under the same ids.
/// Nothing else is shared between runs: the client an agent keeps for its
/// turn lives in its run, whatever the run id.
pub trait IdSource: Send + Sync {
    /// The id of a thread the store creates for an agent given the empty
    /// `thread`: `request` is of rank 0.
    fn thread_id(&self, request: &IdRequest) -> ThreadId;

    /// The id of an agent's turn, which is also the id of the thread message
    /// that holds it: `request` is of rank 1 or more.
    fn turn_id(&self, request: &IdRequest) -> TurnId;
}
