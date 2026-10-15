//! The state of a run and its configuration, checked against a schema.
//!
//! A runner declares its keys on a `SchemaBuilder` and keeps the typed keys
//! it gets (`StateKey<T>`, `ConfigKey<T>`, `T` one of the Rust types of
//! `ValueType`): reads, updates and mappings through them take only values
//! and keys of their kind, so a wrong kind does not compile. What is known
//! only at run time (a state, a configuration or a checkpoint reloaded from
//! JSON, a graph described as data) is read and updated by name, through
//! `Key`, `Kind`, `Value` and `Update`, with the same refusals.

mod apply;
mod config;
mod kind;
mod schema;
#[allow(clippy::module_inception)]
mod state;
mod typed_key;
mod value;
mod value_type;

pub use config::Config;
pub use kind::Kind;
pub use schema::{Schema, SchemaBuilder};
pub use state::{SCHEMA_VERSION, State};
pub use typed_key::{ConfigKey, StateKey};
pub use value::{Finite, Value};
pub use value_type::ValueType;
