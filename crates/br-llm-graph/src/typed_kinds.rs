//! Doctests only (`cfg(doctest)`): a wrong kind through a typed key does
//! not compile. The first program uses every typed form with the right
//! kinds; each of the others changes one key or one value to a wrong kind
//! (rustdoc checks the error code only on nightly; each failure was checked
//! against a crate outside this one when written).
//!
//! ```
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let attempts: i64 = state.get(&count)?;
//! let _ = count.set(attempts + 1)?;
//! let _ = notes.append("seen")?;
//! let _ = chat.input(input);
//! let _ = SubGraph::call(graph)
//!     .input_from(&text, &text)
//!     .config_from(&tone, &tone)
//!     .output_append(&text, &notes)
//!     .end_label_set(&text);
//! let _ = CaptureUpdate::set_reason(&text);
//! let _ = Map::new(&notes, &text, Body);
//! let _ = Limit::config(&width);
//! let _ = GraphBuilder::new(schema.build()).input(&count);
//! # Ok(())
//! # }
//! ```
//!
//! A `Set` takes only a value of the key's type.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = count.set("three")?;# Ok(())
//! # }
//! ```
//!
//! An `Append` takes only a value of the element type.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = notes.append(3)?;# Ok(())
//! # }
//! ```
//!
//! A key that is not a list has no `Append`.
//!
//! ```compile_fail,E0599
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = count.append(1)?;# Ok(())
//! # }
//! ```
//!
//! A key that is not a conversation has no conversation update.
//!
//! ```compile_fail,E0599
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = count.input(input);# Ok(())
//! # }
//! ```
//!
//! A read gives the key's type.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let attempts: String = state.get(&count)?;# Ok(())
//! # }
//! ```
//!
//! Only the Rust types of the kinds can be declared.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let other = schema.state::<u32>("other")?;# Ok(())
//! # }
//! ```
//!
//! A call's input takes a source of its kind.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph).input_from(&text, &count);# Ok(())
//! # }
//! ```
//!
//! A call's configuration key takes a configuration key from `config_from`.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph).config_from(&width, &count);# Ok(())
//! # }
//! ```
//!
//! An output appends only to a list of its kind.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = SubGraph::call(graph).output_append(&count, &notes);# Ok(())
//! # }
//! ```
//!
//! The reason of a failure is a string.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = CaptureUpdate::set_reason(&count);# Ok(())
//! # }
//! ```
//!
//! A map's item has the element type of its list.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = Map::new(&notes, &count, Body);# Ok(())
//! # }
//! ```
//!
//! A limit reads an int configuration key.
//!
//! ```compile_fail,E0308
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = Limit::config(&tone);# Ok(())
//! # }
//! ```
//!
//! A graph's inputs are state keys.
//!
//! ```compile_fail,E0277
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! # use br_llm_messages::{Conversation, UserInput};
//! # struct Body;
//! # impl Node for Body {
//! #     fn run<'a>(&'a self, _: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
//! #         Box::pin(async { Ok(Vec::new()) })
//! #     }
//! # }
//! # fn case(graph: Arc<Graph>, state: &State, input: UserInput) -> Result<(), GraphError> {
//! let mut schema = Schema::builder();
//! let count: StateKey<i64> = schema.state("count")?;
//! let text: StateKey<String> = schema.state("text")?;
//! let notes: StateKey<Vec<String>> = schema.state("notes")?;
//! let chat: StateKey<Conversation> = schema.state("chat")?;
//! let width: ConfigKey<i64> = schema.config("width")?;
//! let tone: ConfigKey<String> = schema.config("tone")?;
//! let _ = GraphBuilder::new(schema.build()).input(&width);# Ok(())
//! # }
//! ```
//!
//! Nothing outside the crate implements `ValueType`: it is sealed.
//!
//! ```compile_fail,E0277
//! use br_llm_graph::{GraphError, Kind, Value, ValueType};
//!
//! struct Money(i64);
//!
//! impl ValueType for Money {
//!     type Read<'a> = i64;
//!     fn kind() -> Kind {
//!         Kind::Int
//!     }
//!     fn read(_value: &Value) -> Option<i64> {
//!         None
//!     }
//!     fn into_value(self) -> Result<Value, GraphError> {
//!         Ok(Value::int(self.0))
//!     }
//! }
//! ```
