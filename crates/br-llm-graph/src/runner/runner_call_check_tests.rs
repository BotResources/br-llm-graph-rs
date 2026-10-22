//! The build checks of a runner call: its target is a runner type that is
//! not blank, and its outputs take a string, each refused with the error
//! writing one would give.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::{CheckSite, GraphBuilder, Node, Target};
use crate::runner::test_support::*;
use crate::runner::{RunnerCall, RunnerOutput, RunnerTarget};
use crate::state::Kind;
use crate::testkit::TestRunners;
use crate::value::EndLabel;

#[test]
fn given_a_blank_target_when_built_then_runner_target_blank() {
    for blank in ["", " ", "\n\t "] {
        let refused = RunnerTarget::new(blank).err().unwrap();
        assert!(
            matches!(refused, GraphError::RunnerTargetBlank),
            "{blank:?}"
        );
        assert_eq!(
            refused.to_string(),
            "the target of a runner call must not be blank"
        );
    }
    // Kept as given, surrounding blanks included.
    assert_eq!(
        RunnerTarget::new(" ask_a_document ").unwrap().as_str(),
        " ask_a_document "
    );
}

#[test]
fn given_outputs_that_cannot_take_a_string_when_built_then_the_error_writing_one_would_give() {
    let port = Arc::new(TestRunners::new());
    let refusal = |output: RunnerOutput, thread: Option<RunnerOutput>| {
        RunnerCall {
            thread,
            ..ask(&port, output)
        }
        .check(&schema(), CheckSite::Graph)
        .err()
        .unwrap()
    };
    for thread in [false, true] {
        let place = |output: RunnerOutput| match thread {
            false => refusal(output, None),
            true => refusal(set_answer(), Some(output)),
        };
        assert!(matches!(
            place(RunnerOutput::Set(skey("missing"))),
            GraphError::UnknownKey { key: found } if found == key("missing")
        ));
        assert!(matches!(
            place(RunnerOutput::Set(skey("count"))),
            GraphError::KindMismatch { key: found, expected: Kind::Int, found: "str" }
                if found == key("count")
        ));
        assert!(matches!(
            place(RunnerOutput::Append(skey("answer"))),
            GraphError::AppendNotList { key: found } if found == key("answer")
        ));
        assert!(matches!(
            place(RunnerOutput::Append(skey("counts"))),
            GraphError::KindMismatch { key: found, expected: Kind::Int, found: "str" }
                if found == key("counts")
        ));
    }
}

#[test]
fn given_outputs_that_take_a_string_when_the_graph_is_built_then_it_is() {
    let port = Arc::new(TestRunners::new());
    let node = RunnerCall {
        thread: Some(RunnerOutput::Append(skey("threads"))),
        ..ask(&port, set_answer())
    };
    assert!(
        GraphBuilder::new(schema())
            .entry(nid("ask"))
            .node(nid("ask"), node)
            .edge(
                nid("ask"),
                crate::graph::Always(Target::End(EndLabel::new("done").unwrap()))
            )
            .build()
            .is_ok()
    );
}

#[test]
fn given_a_refused_output_when_the_graph_is_built_then_invalid_node_naming_the_runner_call() {
    let port = Arc::new(TestRunners::new());
    let built = GraphBuilder::new(schema())
        .entry(nid("ask"))
        .node(nid("ask"), ask(&port, RunnerOutput::Set(skey("missing"))))
        .edge(
            nid("ask"),
            crate::graph::Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build();
    assert!(matches!(
        built,
        Err(GraphError::InvalidNode { node, source })
            if node == nid("ask") && matches!(*source, GraphError::UnknownKey { .. })
    ));
}
