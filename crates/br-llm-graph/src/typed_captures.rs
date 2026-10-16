//! Doctests only (`cfg(doctest)`): a capture update from a key or a fixed
//! value, and a count read from the configuration, take only keys of their
//! kind. The first program uses each with the right kinds; each of the
//! others is that program's line with one key of another kind, nothing else
//! changed (stable rustdoc does not check the error code, so the twins make
//! sure a negative fails on its kind and nothing else).
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
//! let _ = CaptureUpdate::set_from(&text, &text);
//! let _ = CaptureUpdate::set_value(&count, 3)?;
//! let _ = CaptureUpdate::append_from(&notes, &text);
//! let _ = CaptureUpdate::append_value(&counts, 3)?;
//! let _ = Count::config(&width);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A capture sets a key from a key of its kind.
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
//! let _ = CaptureUpdate::set_from(&text, &count);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A capture sets a key to a fixed value of its kind.
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
//! let _ = CaptureUpdate::set_value(&text, 3)?;
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A capture appends a key of the list's element kind.
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
//! let _ = CaptureUpdate::append_from(&notes, &count);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A capture appends a fixed value of the list's element kind.
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
//! let _ = CaptureUpdate::append_value(&notes, 3)?;
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
//!
//! A count reads an int configuration key.
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
//! let _ = Count::config(&tone);
//! # let _ = graph;
//! # Ok(())
//! # }
//! ```
