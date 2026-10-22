use crate::error::GraphError;

impl std::error::Error for GraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GraphError::NodeFailed { source, .. } => Some(source),
            GraphError::Message(error) => Some(error),
            GraphError::Model(error) => Some(error),
            GraphError::Request(error) => Some(error),
            GraphError::SubGraphFailed { source } => Some(source.as_ref()),
            GraphError::InvalidNode { source, .. } => Some(source.as_ref()),
            GraphError::Thread { source } => Some(source.as_ref()),
            GraphError::Runner { source } => Some(source.as_ref()),
            GraphError::Identifier { .. }
            | GraphError::FloatNotFinite
            | GraphError::ThreadId { .. }
            | GraphError::MissingKey { .. }
            | GraphError::UnknownKey { .. }
            | GraphError::KindMismatch { .. }
            | GraphError::SchemaMismatch { .. }
            | GraphError::SetConflict { .. }
            | GraphError::AppendNotList { .. }
            | GraphError::NotConversation { .. }
            | GraphError::DuplicateNode { .. }
            | GraphError::MissingEntry
            | GraphError::UnknownEntry { .. }
            | GraphError::NodeWithoutEdge { .. }
            | GraphError::EdgeFromUnknownNode { .. }
            | GraphError::DuplicateEdge { .. }
            | GraphError::EdgeToUnknownNode { .. }
            | GraphError::UnknownNode { .. }
            | GraphError::EmptyEdge { .. }
            | GraphError::UndeclaredTarget { .. }
            | GraphError::MapKeyMismatch { .. }
            | GraphError::AmbiguousEnd { .. }
            | GraphError::EmptyCursor
            | GraphError::StateSchemaMismatch
            | GraphError::ConfigSchemaMismatch
            | GraphError::ToolNotCovered { .. }
            | GraphError::ToolCoveredTwice { .. }
            | GraphError::ToolNotDeclared { .. }
            | GraphError::EnabledKeyMismatch { .. }
            | GraphError::Structured { .. }
            | GraphError::LimitKeyMismatch { .. }
            | GraphError::LimitNotPositive { .. }
            | GraphError::CountKeyMismatch { .. }
            | GraphError::CountNegative { .. }
            | GraphError::SameAuthor { .. }
            | GraphError::VerdictShapeMismatch { .. }
            | GraphError::KeyCollision { .. }
            | GraphError::FlagKeyMismatch { .. }
            | GraphError::ThreadNotFound { .. }
            | GraphError::OpeningNotInputs
            | GraphError::RunnerFailed { .. }
            | GraphError::RunnerTargetBlank
            | GraphError::ToolLimitReached { .. }
            | GraphError::DuplicateInput { .. }
            | GraphError::DuplicateOutput { .. }
            | GraphError::MissingInput { .. }
            | GraphError::NotAnInput { .. }
            | GraphError::InputGivenTwice { .. }
            | GraphError::InvalidOccurrence { .. }
            | GraphError::SubGraphNotAnInput { .. }
            | GraphError::SubGraphInputTwice { .. }
            | GraphError::SubGraphInputUnmapped { .. }
            | GraphError::SubGraphSourceMismatch { .. }
            | GraphError::SubGraphNotAConfig { .. }
            | GraphError::SubGraphConfigTwice { .. }
            | GraphError::SubGraphConfigUnmapped { .. }
            | GraphError::SubGraphConfigMismatch { .. }
            | GraphError::SubGraphNotAnOutput { .. }
            | GraphError::SubGraphTargetMismatch { .. }
            | GraphError::SubGraphSetTwice { .. }
            | GraphError::SubGraphSuspended
            | GraphError::MapBodyNotAppend { .. }
            | GraphError::MapBodySet { .. }
            | GraphError::MapWithoutOccurrence
            | GraphError::CaptureMismatch { .. }
            | GraphError::CaptureSetTwice { .. } => None,
        }
    }
}
