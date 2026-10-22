//! A call's mappings and captures through typed keys: each takes only keys
//! of one kind, so a wrong kind does not compile.
//!
//! The build still checks every mapping against the two graphs' signatures
//! and schemas (`SubGraphNotAnInput`, `SubGraphSourceMismatch`, ...), so a
//! typed key of another schema is refused there as a key by name is.

use crate::error::GraphError;
use crate::graph::output::Output;
use crate::graph::subgraph::{CaptureSource, CaptureUpdate, Input, SubGraph};
use crate::state::{ConfigKey, StateKey, ValueType};

impl SubGraph {
    /// Maps the child's input `child` from the caller's state key `from`.
    pub fn input_from<T: ValueType>(self, child: &StateKey<T>, from: &StateKey<T>) -> Self {
        self.input(child.key().clone(), Input::From(from.key().clone()))
    }

    /// Maps the child's input `child` from the caller's configuration key
    /// `from`.
    pub fn input_from_config<T: ValueType>(self, child: &StateKey<T>, from: &ConfigKey<T>) -> Self {
        self.input(child.key().clone(), Input::Config(from.key().clone()))
    }

    /// Maps the child's input `child` to the fixed value `value`. A float that
    /// is not finite is refused (`FloatNotFinite`).
    pub fn input_value<T: ValueType>(
        self,
        child: &StateKey<T>,
        value: impl Into<T>,
    ) -> Result<Self, GraphError> {
        let value = value.into().into_value()?;
        Ok(self.input(child.key().clone(), Input::Const(value)))
    }

    /// Maps the child's configuration key `child` from the caller's
    /// configuration key `from`.
    pub fn config_from<T: ValueType>(self, child: &ConfigKey<T>, from: &ConfigKey<T>) -> Self {
        self.config(child.key().clone(), Input::Config(from.key().clone()))
    }

    /// Maps the child's configuration key `child` from the caller's state key
    /// `from`.
    pub fn config_from_state<T: ValueType>(self, child: &ConfigKey<T>, from: &StateKey<T>) -> Self {
        self.config(child.key().clone(), Input::From(from.key().clone()))
    }

    /// Maps the child's configuration key `child` to the fixed value `value`.
    /// A float that is not finite is refused (`FloatNotFinite`).
    pub fn config_value<T: ValueType>(
        self,
        child: &ConfigKey<T>,
        value: impl Into<T>,
    ) -> Result<Self, GraphError> {
        let value = value.into().into_value()?;
        Ok(self.config(child.key().clone(), Input::Const(value)))
    }

    /// Sets the caller's key `to` to the child's output `child`.
    pub fn output_set<T: ValueType>(self, child: &StateKey<T>, to: &StateKey<T>) -> Self {
        self.output(child.key().clone(), Output::Set(to.key().clone()))
    }

    /// Appends the child's output `child` to the caller's list `to`.
    pub fn output_append<T: ValueType>(self, child: &StateKey<T>, to: &StateKey<Vec<T>>) -> Self {
        self.output(child.key().clone(), Output::Append(to.key().clone()))
    }

    /// Sets the caller's string key `to` to the label the child ended with.
    pub fn end_label_set(self, to: &StateKey<String>) -> Self {
        self.output_end_label(Output::Set(to.key().clone()))
    }

    /// Appends the label the child ended with to the caller's list of strings
    /// `to`.
    pub fn end_label_append(self, to: &StateKey<Vec<String>>) -> Self {
        self.output_end_label(Output::Append(to.key().clone()))
    }
}

impl CaptureUpdate {
    /// Sets the caller's key `target` to the fixed value `value`. A float that
    /// is not finite is refused (`FloatNotFinite`).
    pub fn set_value<T: ValueType>(
        target: &StateKey<T>,
        value: impl Into<T>,
    ) -> Result<Self, GraphError> {
        let value = value.into().into_value()?;
        Ok(CaptureUpdate::Set(
            target.key().clone(),
            CaptureSource::Const(value),
        ))
    }

    /// Sets the caller's key `target` to the value of the caller's key
    /// `source` (in a map body, of the item's state).
    pub fn set_from<T: ValueType>(target: &StateKey<T>, source: &StateKey<T>) -> Self {
        CaptureUpdate::Set(
            target.key().clone(),
            CaptureSource::From(source.key().clone()),
        )
    }

    /// Sets the caller's string key `target` to the reason: the message of
    /// the failure.
    pub fn set_reason(target: &StateKey<String>) -> Self {
        CaptureUpdate::Set(target.key().clone(), CaptureSource::Reason)
    }

    /// Appends the fixed value `value` to the caller's list `target`. A float
    /// that is not finite is refused (`FloatNotFinite`).
    pub fn append_value<T: ValueType>(
        target: &StateKey<Vec<T>>,
        value: impl Into<T>,
    ) -> Result<Self, GraphError> {
        let value = value.into().into_value()?;
        Ok(CaptureUpdate::Append(
            target.key().clone(),
            CaptureSource::Const(value),
        ))
    }

    /// Appends the value of the caller's key `source` (in a map body, of the
    /// item's state) to the caller's list `target`.
    pub fn append_from<T: ValueType>(target: &StateKey<Vec<T>>, source: &StateKey<T>) -> Self {
        CaptureUpdate::Append(
            target.key().clone(),
            CaptureSource::From(source.key().clone()),
        )
    }

    /// Appends the reason, the message of the failure, to the caller's list of
    /// strings `target`.
    pub fn append_reason(target: &StateKey<Vec<String>>) -> Self {
        CaptureUpdate::Append(target.key().clone(), CaptureSource::Reason)
    }
}
