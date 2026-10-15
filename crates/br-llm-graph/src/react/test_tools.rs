//! The tools of the react tests: echo, tally (with an update), scope (reads
//! the state), write_file (counts its calls) and strict (parses its
//! arguments).

use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_clients_core::{Parameters, ToolSpec};
use br_llm_messages::{Text, ToolName, ToolResultBlock};
use serde_json::Value as Json;

use crate::react::test_support::key;
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::state::{Config, State, Value};
use crate::update::Update;

/// The spec of a test tool without parameters.
pub(crate) fn spec(name: &str) -> ToolSpec {
    ToolSpec::new(
        ToolName::new(name).unwrap(),
        format!("the {name} tool"),
        Parameters::default(),
    )
    .unwrap()
}

pub(crate) struct EchoTool;

impl Tool for EchoTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("echo"));
        &SPEC
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("echoed").unwrap(),
            )]))
        })
    }
}

pub(crate) struct TallyTool;

impl Tool for TallyTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("tally"));
        &SPEC
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(
                ToolOutput::text(vec![ToolResultBlock::text(Text::new("tallied").unwrap())])
                    .with_updates(vec![Update::Append {
                        key: key("log"),
                        value: Value::str("tallied"),
                    }]),
            )
        })
    }
}

/// Answers with the value of the string state key `scope`.
pub(crate) struct ScopeTool;

impl Tool for ScopeTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("scope"));
        &SPEC
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            let scope = state.str(&key("scope"))?;
            Ok(ToolOutput::text(vec![ToolResultBlock::text(Text::new(
                format!("scope is {scope}"),
            )?)]))
        })
    }
}

/// Counts its calls.
#[derive(Default)]
pub(crate) struct WriteFileTool {
    pub(crate) calls: AtomicUsize,
}

impl Tool for WriteFileTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("write_file"));
        &SPEC
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("written").unwrap(),
            )]))
        })
    }
}

pub(crate) struct StrictTool;

impl Tool for StrictTool {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("strict"));
        &SPEC
    }
    fn call<'a>(
        &'a self,
        arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            let _parsed: StrictArgs = serde_json::from_value(arguments)?;
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("ok").unwrap(),
            )]))
        })
    }
}

#[derive(serde::Deserialize)]
struct StrictArgs {
    #[allow(dead_code)]
    n: i64,
}
