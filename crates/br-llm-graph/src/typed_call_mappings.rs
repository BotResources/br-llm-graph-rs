//! Doctests only (`cfg(doctest)`): the typed mappings of a call that
//! `typed_mappings` does not cover take only keys of one kind. The first
//! program uses each with the right kinds; each of the others is that
//! program's line with one key of another kind, nothing else changed
//! (stable rustdoc does not check the error code, so the twins make sure a
//! negative fails on its kind and nothing else).
//!
//! ```
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let call = SubGraph::call(graph.clone())
//!     .output_set(&text, &text)
//!     .input_from_config(&text, &tone)
//!     .config_from_state(&tone, &text)
//!     .input_value(&count, 3)?
//!     .config_value(&width, 3)?
//!     .end_label_set(&text)
//!     .end_label_append(&notes);
//! # let _ = call;
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! An output is set into a key of its kind.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).output_set(&text, &count);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A child's input is mapped from a configuration key of its kind.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).input_from_config(&text, &width);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A child's configuration key is mapped from a state key of its kind.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).config_from_state(&tone, &count);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A child's input takes a fixed value of its kind.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).input_value(&text, 3)?;
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A child's configuration key takes a fixed value of its kind.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).config_value(&tone, 3)?;
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! The end label is set into a string key.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).end_label_set(&count);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! The end label is appended to a list of strings.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # fn case(graph: Arc<Graph>) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let counts: StateKey<Vec<i64>> = schema.state("counts")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph.clone()).end_label_append(&counts);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
