//! Doctests only (`cfg(doctest)`): the enums a runner builds to configure a
//! node, `Judgment`, `LlmRequest`, `TextOutput`, `Ruling` and
//! `CriticHistory`, the verdict reader's `Reading`, and those a runner port
//! builds, `RunnerReply` and `AnswerBlock`, may gain variants in a later
//! version. A host builds their variants as before, and a `match` outside
//! the crate needs a wildcard arm: one without it does not compile (E0004;
//! rustdoc checks the code itself only on nightly).
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
//!
//! The enums of 0.5 a runner builds, matched with a wildcard arm.
//!
//! ```
//! use br_llm_graph::{CriticHistory, Ruling, Schema};
//! use br_llm_messages::Text;
//!
//! let mut schema = Schema::builder();
//! let rulings = [Ruling::Pass, Ruling::Revise(Text::new("Longer.").unwrap())];
//! let histories = [
//!     CriticHistory::History,
//!     CriticHistory::Latest,
//!     CriticHistory::Brief(schema.state("brief").unwrap()),
//! ];
//! let passes = rulings
//!     .iter()
//!     .filter(|ruling| match ruling {
//!         Ruling::Pass => true,
//!         Ruling::Revise(_) => false,
//!         _ => false,
//!     })
//!     .count();
//! let briefs = histories
//!     .iter()
//!     .filter(|history| match history {
//!         CriticHistory::Brief(_) => true,
//!         CriticHistory::History | CriticHistory::Latest | CriticHistory::Given(_) => false,
//!         _ => false,
//!     })
//!     .count();
//! assert_eq!((passes, briefs), (1, 1));
//! ```
//!
//! ```compile_fail,E0004
//! fn pass(ruling: &br_llm_graph::Ruling) -> bool {
//!     match ruling {
//!         br_llm_graph::Ruling::Pass => true,
//!         br_llm_graph::Ruling::Revise(_) => false,
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! fn brief(history: &br_llm_graph::CriticHistory) -> bool {
//!     use br_llm_graph::CriticHistory;
//!     match history {
//!         CriticHistory::Brief(_) => true,
//!         CriticHistory::History | CriticHistory::Latest | CriticHistory::Given(_) => false,
//!     }
//! }
//! ```
//!
//! The enums of 0.7 a runner or its runner port builds, matched with a
//! wildcard arm.
//!
//! ```
//! use br_llm_graph::{AnswerBlock, RunnerAnswer, RunnerReply, Schema, TextOutput, ThreadId};
//! use br_llm_messages::Text;
//!
//! let mut schema = Schema::builder();
//! let outputs = [
//!     TextOutput::Set(schema.state("answer").unwrap()),
//!     TextOutput::Append(schema.state("answers").unwrap()),
//! ];
//! let blocks = vec![
//!     AnswerBlock::Text(Text::new("Two documents answer.").unwrap()),
//!     AnswerBlock::Structured(serde_json::json!({"document_ids": ["d1", "d2"]})),
//! ];
//! let thread = ThreadId::new("01900000-0000-7000-8000-000000000001").unwrap();
//! let replies = [
//!     RunnerReply::Answered(RunnerAnswer::new(blocks.clone(), thread)),
//!     RunnerReply::Failed { reason: "the child job failed".to_owned() },
//! ];
//! let appends = outputs
//!     .iter()
//!     .filter(|output| match output {
//!         TextOutput::Append(_) => true,
//!         TextOutput::Set(_) => false,
//!         _ => false,
//!     })
//!     .count();
//! let answered = replies
//!     .iter()
//!     .filter(|reply| match reply {
//!         RunnerReply::Answered(_) => true,
//!         RunnerReply::Failed { .. } => false,
//!         _ => false,
//!     })
//!     .count();
//! let structured = blocks
//!     .iter()
//!     .filter(|block| match block {
//!         AnswerBlock::Structured(_) => true,
//!         AnswerBlock::Text(_) => false,
//!         _ => false,
//!     })
//!     .count();
//! assert_eq!((appends, answered, structured), (1, 1, 1));
//! ```
//!
//! ```compile_fail,E0004
//! fn answered(reply: &br_llm_graph::RunnerReply) -> bool {
//!     match reply {
//!         br_llm_graph::RunnerReply::Answered(_) => true,
//!         br_llm_graph::RunnerReply::Failed { .. } => false,
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! fn structured(block: &br_llm_graph::AnswerBlock) -> bool {
//!     match block {
//!         br_llm_graph::AnswerBlock::Structured(_) => true,
//!         br_llm_graph::AnswerBlock::Text(_) => false,
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! fn appends(output: &br_llm_graph::TextOutput) -> bool {
//!     match output {
//!         br_llm_graph::TextOutput::Append(_) => true,
//!         br_llm_graph::TextOutput::Set(_) => false,
//!     }
//! }
//! ```
//!
//! The enum of 0.7 a runner builds, what its verdict reader gives, matched
//! with a wildcard arm.
//!
//! ```
//! use br_llm_graph::{Reading, Verdict};
//!
//! let readings = [
//!     Reading::Verdict(Verdict { is_valid: true, message: String::new() }),
//!     Reading::Unreadable { reason: "the grade is missing".to_owned() },
//! ];
//! let unreadable = readings
//!     .iter()
//!     .filter(|reading| match reading {
//!         Reading::Unreadable { .. } => true,
//!         Reading::Verdict(_) => false,
//!         _ => false,
//!     })
//!     .count();
//! assert_eq!(unreadable, 1);
//! ```
//!
//! ```compile_fail,E0004
//! fn unreadable(reading: &br_llm_graph::Reading) -> bool {
//!     match reading {
//!         br_llm_graph::Reading::Unreadable { .. } => true,
//!         br_llm_graph::Reading::Verdict(_) => false,
//!     }
//! }
//! ```
