//! Doctests only (`cfg(doctest)`): `Judgment` and `LlmRequest` may gain
//! variants in a later version. A host builds their variants as before, and
//! a `match` outside the crate needs a wildcard arm: one without it does not
//! compile (E0004; rustdoc checks the code itself only on nightly).
//!
//! ```
//! use br_llm_graph::{History, Judgment, LlmRequest, Prompt, Schema};
//!
//! let judgments = [
//!     Judgment::Accept,
//!     Judgment::Refuse { reason: "outside notes/".to_owned() },
//! ];
//! let mut schema = Schema::builder();
//! let requests = [
//!     LlmRequest::Text(Prompt::text("Summarize.")),
//!     LlmRequest::Conversation {
//!         key: schema.state("chat").unwrap(),
//!         author: br_llm_messages::Author::new("agent", "agent").unwrap(),
//!     },
//!     LlmRequest::History(History::value(Vec::new())),
//! ];
//! let refused = judgments
//!     .iter()
//!     .filter(|judgment| match judgment {
//!         Judgment::Refuse { .. } => true,
//!         Judgment::Accept => false,
//!         _ => false,
//!     })
//!     .count();
//! let texts = requests
//!     .iter()
//!     .filter(|request| match request {
//!         LlmRequest::Text(_) => true,
//!         LlmRequest::Conversation { .. } | LlmRequest::History(_) => false,
//!         _ => false,
//!     })
//!     .count();
//! assert_eq!((refused, texts), (1, 1));
//! ```
//!
//! ```compile_fail,E0004
//! fn refused(judgment: &br_llm_graph::Judgment) -> bool {
//!     match judgment {
//!         br_llm_graph::Judgment::Accept => false,
//!         br_llm_graph::Judgment::Refuse { .. } => true,
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! fn text(request: &br_llm_graph::LlmRequest) -> bool {
//!     match request {
//!         br_llm_graph::LlmRequest::Text(_) => true,
//!         br_llm_graph::LlmRequest::Conversation { .. } => false,
//!         br_llm_graph::LlmRequest::History(_) => false,
//!     }
//! }
//! ```
