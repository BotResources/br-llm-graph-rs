//! How a subject was given, as a graph's description writes it: never the
//! value itself.

/// How a subject of a model-calling node was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// One value given when the node is built, the same on every call:
    /// `Given::value`, `Given::text`, `Given::client`.
    Fixed,
    /// A value the library computes: a text read from a key
    /// (`Given::from_state`, `from_state_list`, `from_config`,
    /// `from_config_list`), a schedule of clients (`Given::schedule`,
    /// `Given::ending_with`), or a node's default (its default history, the
    /// critic's histories, a default text).
    ReadyMade,
    /// A function the runner wrote (`Given::from_fn`).
    Function,
}

impl Origin {
    /// Its name in a description.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Origin::Fixed => "fixed",
            Origin::ReadyMade => "ready_made",
            Origin::Function => "function",
        }
    }
}
