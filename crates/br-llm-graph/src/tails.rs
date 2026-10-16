//! Doctests only (`cfg(doctest)`): a schedule ending with a tail exists only
//! for the two seats of the generator-critic, the nodes whose context holds
//! a budget (`Budgeted`). The first program builds a tail for each seat and a
//! schedule from the start for every scheduled node; each of the others asks
//! a tail of a node without a budget, which does not compile (E0277, the node is not `Budgeted`;
//! rustdoc checks the code itself only on nightly).
//!
//! ```
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(primary: Arc<dyn br_llm_clients_core::Client>, tail: Arc<dyn br_llm_clients_core::Client>) {
//! let _: Model<GeneratorSeat> = Model::ending_with(primary.clone(), vec![tail.clone()]);
//! let _: Model<CriticSeat> = Model::ending_with(primary.clone(), vec![tail.clone()]);
//! let _: Model<ReactAgent> = Model::schedule(primary.clone(), vec![tail.clone()]);
//! let _: Model<GeneratorSeat> = Model::schedule(primary.clone(), vec![tail.clone()]);
//! let _: Model<CriticSeat> = Model::schedule(primary, vec![tail]);
//! # }
//! ```
//!
//! A ReAct agent has no budget, so no tail.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(primary: Arc<dyn br_llm_clients_core::Client>, tail: Arc<dyn br_llm_clients_core::Client>) {
//! let _: Model<ReactAgent> = Model::ending_with(primary, vec![tail]);
//! # }
//! ```
//!
//! An LLM call has no budget either.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(primary: Arc<dyn br_llm_clients_core::Client>, tail: Arc<dyn br_llm_clients_core::Client>) {
//! let _: Model<LlmCall> = Model::ending_with(primary, vec![tail]);
//! # }
//! ```
