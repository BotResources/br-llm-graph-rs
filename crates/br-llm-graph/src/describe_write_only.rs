//! Doctests only (`cfg(doctest)`): a description is written, never
//! reloaded. `GraphDescription` and `NodeDescription` implement `Serialize`
//! and not `Deserialize`, so no graph can be built from one: reading one
//! back does not compile (E0277).
//!
//! ```compile_fail,E0277
//! let _ = serde_json::from_str::<br_llm_graph::GraphDescription>("{}");
//! ```
//!
//! ```compile_fail,E0277
//! let _ = serde_json::from_str::<br_llm_graph::NodeDescription>("{}");
//! ```
//!
//! Writing one compiles:
//!
//! ```
//! fn written(description: &br_llm_graph::GraphDescription) -> String {
//!     serde_json::to_string(description).unwrap()
//! }
//! ```
