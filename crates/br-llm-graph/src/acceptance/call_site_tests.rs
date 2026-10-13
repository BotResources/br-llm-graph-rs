use std::sync::Arc;

use super::support::*;
use crate::{CaptureSource, CaptureUpdate, GraphError, Input, OnFailure, Output};

fn refused(body: crate::SubGraph) -> GraphError {
    match caller(body) {
        Err(GraphError::InvalidNode { node, source }) => {
            assert_eq!(node, nid("each"));
            *source
        }
        Err(other) => panic!("expected the map node to be refused, got {other}"),
        Ok(_) => panic!("expected the map node to be refused"),
    }
}

#[test]
fn given_wrong_call_sites_when_built_then_each_is_refused_with_its_own_error() {
    let probe = Arc::new(Probe::default());
    assert!(caller(per_item(&probe)).is_ok());

    let set_in_map = per_item(&probe).output(key("analysis"), Output::Set(key("item")));
    assert!(matches!(refused(set_in_map), GraphError::MapBodySet { .. }));

    let set_capture = per_item(&probe).on_failure(OnFailure::Capture(vec![CaptureUpdate::Set(
        key("item"),
        CaptureSource::Reason,
    )]));
    assert!(matches!(
        refused(set_capture),
        GraphError::MapBodySet { .. }
    ));

    let wrong_list = per_item(&probe).output(key("attempts"), Output::Append(key("analyses")));
    assert!(matches!(
        refused(wrong_list),
        GraphError::SubGraphTargetMismatch { .. }
    ));

    let unknown_source = crate::SubGraph::call(per_item(&probe).graph().clone())
        .input(key("item"), Input::From(key("document")))
        .output(key("analysis"), Output::Append(key("analyses")));
    assert!(matches!(
        refused(unknown_source),
        GraphError::SubGraphSourceMismatch { .. }
    ));

    let unmapped = crate::SubGraph::call(per_item(&probe).graph().clone())
        .output(key("analysis"), Output::Append(key("analyses")));
    assert!(matches!(
        refused(unmapped),
        GraphError::SubGraphInputUnmapped { .. }
    ));

    let private_output = per_item(&probe).output(key("text"), Output::Append(key("analyses")));
    assert!(matches!(
        refused(private_output),
        GraphError::SubGraphNotAnOutput { .. }
    ));

    let wrong_capture =
        per_item(&probe).on_failure(OnFailure::Capture(vec![CaptureUpdate::Append(
            key("attempts"),
            CaptureSource::Reason,
        )]));
    assert!(matches!(
        refused(wrong_capture),
        GraphError::CaptureMismatch { .. }
    ));
}
