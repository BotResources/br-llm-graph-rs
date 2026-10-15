use std::marker::PhantomData;

use crate::state::value_type::ValueType;
use crate::value::Key;

/// A state key declared by a schema, carrying its kind as the Rust type `T`
/// (see `ValueType`).
///
/// `SchemaBuilder::state` gives it. Reading through it (`State::get`) gives
/// a value of its type, a `Set` through it (`StateKey::set`) takes only a
/// value of its type, an `Append` (`StateKey::append`) only a value of its
/// element type, and a mapping between two typed keys (a call's inputs,
/// outputs and captures, a map's list and item) takes only keys of one kind:
/// a wrong kind does not compile. A float is still checked when its value is
/// built: it must be finite (`FloatNotFinite`).
///
/// A runner declares its keys once, gives them to the nodes that read or
/// write them, and names them in the graph's declarations:
///
/// ```
/// use br_llm_graph::{
///     Always, Config, Context, EndLabel, GraphBuilder, Node, NodeFuture, NodeId, Schema,
///     State, StateKey, Target,
/// };
///
/// /// Counts an attempt and notes it.
/// struct Attempt {
///     attempts: StateKey<i64>,
///     notes: StateKey<Vec<String>>,
/// }
///
/// impl Node for Attempt {
///     fn run<'a>(&'a self, state: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
///         Box::pin(async move {
///             let attempt = state.get(&self.attempts)? + 1;
///             Ok(vec![
///                 self.attempts.set(attempt)?,
///                 self.notes.append(format!("attempt {attempt}"))?,
///             ])
///         })
///     }
/// }
///
/// # fn main() -> Result<(), br_llm_graph::GraphError> {
/// let mut schema = Schema::builder();
/// let attempts: StateKey<i64> = schema.state("attempts")?;
/// let notes: StateKey<Vec<String>> = schema.state("notes")?;
/// let attempt = NodeId::new("attempt")?;
/// let graph = GraphBuilder::new(schema.build())
///     .input(&attempts)
///     .output(&notes)
///     .entry(attempt.clone())
///     .node(attempt.clone(), Attempt { attempts: attempts.clone(), notes: notes.clone() })
///     .edge(attempt, Always(Target::End(EndLabel::new("done")?)))
///     .build()?;
///
/// let mut state = graph.start_state([attempts.given(2)?])?;
/// assert_eq!(state.get(&attempts)?, 2);
/// state.apply_batch(&[attempts.set(3)?, notes.append("attempt 3")?])?;
/// assert_eq!(state.get(&notes)?, vec!["attempt 3"]);
/// # Ok(())
/// # }
/// ```
///
/// A key whose name or kind is known only at run time (a state, a
/// configuration or a checkpoint reloaded from JSON, a graph described as
/// data) is read and updated by name, through `Key` and `Update`, with the
/// same refusals.
pub struct StateKey<T> {
    key: Key,
    kind: PhantomData<fn() -> T>,
}

/// A configuration key declared by a schema, carrying its kind as the Rust
/// type `T`.
///
/// `SchemaBuilder::config` gives it; reading through it (`Config::get`)
/// gives a value of its type. A configuration is read-only: nothing is set
/// through it.
pub struct ConfigKey<T> {
    key: Key,
    kind: PhantomData<fn() -> T>,
}

macro_rules! typed_key {
    ($name:ident) => {
        impl<T: ValueType> $name<T> {
            /// The typed key of a key the schema declares with the kind of
            /// `T`; the declaration makes it.
            pub(crate) fn declared(key: Key) -> Self {
                Self {
                    key,
                    kind: PhantomData,
                }
            }

            /// The key, by name.
            pub fn key(&self) -> &Key {
                &self.key
            }

            /// The name of the key.
            pub fn as_str(&self) -> &str {
                self.key.as_str()
            }
        }

        impl<T> Clone for $name<T> {
            fn clone(&self) -> Self {
                Self {
                    key: self.key.clone(),
                    kind: PhantomData,
                }
            }
        }

        impl<T> PartialEq for $name<T> {
            fn eq(&self, other: &Self) -> bool {
                self.key == other.key
            }
        }

        impl<T> Eq for $name<T> {}

        impl<T> std::hash::Hash for $name<T> {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.key.hash(state);
            }
        }

        impl<T: ValueType> std::fmt::Debug for $name<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("key", &self.key)
                    .field("kind", &T::kind())
                    .finish()
            }
        }

        impl<T> std::fmt::Display for $name<T> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.key, f)
            }
        }
    };
}

typed_key!(StateKey);
typed_key!(ConfigKey);

/// A typed state key, by name: what a graph's input and output declarations
/// take (`GraphBuilder::input`, `GraphBuilder::output`).
impl<T> From<&StateKey<T>> for Key {
    fn from(key: &StateKey<T>) -> Self {
        key.key.clone()
    }
}

impl<T> From<StateKey<T>> for Key {
    fn from(key: StateKey<T>) -> Self {
        key.key
    }
}
