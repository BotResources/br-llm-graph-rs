//! What the react module's nodes say of themselves in a description: how
//! each subject is given, never its value, and the tools by name.

use crate::describe::NodeDescription;
use crate::describe::shape::{
    AuthorShape, CheckShape, CriticShape, LlmCallShape, ModelCallShape, RequestShape, Shape,
    ToolsShape,
};
use crate::graph::Output;
use crate::react::check::CheckNode;
use crate::react::critique::CritiqueNode;
use crate::react::given::{History, LoopNode, Origin};
use crate::react::llm_call::{LlmCall, LlmOutput, LlmRequest};
use crate::react::llm_node::LlmNode;
use crate::react::seats::CriticHistory;
use crate::react::tool::Tool;
use crate::react::tool_node::ToolNode;
use crate::react::verdict::VerdictForm;

/// The names of `tools`, in order.
fn names(tools: &[std::sync::Arc<dyn Tool>]) -> Vec<String> {
    tools
        .iter()
        .map(|tool| tool.spec().name().as_str().to_owned())
        .collect()
}

/// How a history is given: by default (`None`), the node's own, ready-made.
fn history_origin<N: crate::react::given::ModelNode>(history: Option<&History<N>>) -> Origin {
    history.map_or(Origin::ReadyMade, History::origin)
}

impl LlmCall {
    /// `llm_call`: how its model and system are given, its request and its
    /// output.
    pub(crate) fn description(&self) -> NodeDescription {
        let request = match &self.request {
            LlmRequest::Text(text) => RequestShape::Text(text.origin()),
            LlmRequest::Conversation { key, author } => RequestShape::Conversation {
                key: key.key().clone(),
                author: AuthorShape::of(author),
            },
            LlmRequest::History(history) => RequestShape::History(history.origin()),
        };
        let output = match &self.output {
            LlmOutput::Set(key) => Output::Set(key.key().clone()),
            LlmOutput::Append(key) => Output::Append(key.key().clone()),
        };
        NodeDescription::of(Shape::LlmCall(LlmCallShape {
            model: self.model.origin(),
            system: self.system.origin(),
            request,
            output,
        }))
    }
}

impl<N: LoopNode> LlmNode<N> {
    /// `model_call`: the seat of the ReAct loop.
    pub(crate) fn description(&self) -> NodeDescription {
        NodeDescription::of(Shape::ModelCall(Box::new(ModelCallShape {
            author: AuthorShape::of(&self.author),
            model: self.model.origin(),
            system: self.system.origin(),
            history: history_origin(self.history.as_ref()),
            tools: names(&self.tools),
            enabled: self.enabled.clone(),
            tool_concurrency: self.tool_concurrency.clone(),
            round_limit: self.round_limit.clone(),
            critic: None,
        })))
    }
}

impl ToolNode {
    /// `tools`: the names of the tools this node runs.
    pub(crate) fn description(&self) -> NodeDescription {
        NodeDescription::of(Shape::Tools(ToolsShape {
            tools: names(&self.tools),
        }))
    }
}

impl CritiqueNode {
    /// `model_call`: the critic's seat, which has no tools, and what the
    /// generator-critic's routing and verdict read: the critique budget,
    /// `final_generation`, the verdict retries, the review request and the
    /// verdict's shape.
    pub(crate) fn description(&self) -> NodeDescription {
        let history = match &self.view.history {
            CriticHistory::History | CriticHistory::Latest | CriticHistory::Brief(_) => {
                Origin::ReadyMade
            }
            CriticHistory::Given(history) => history.origin(),
        };
        let duo = &self.view.duo;
        NodeDescription::of(Shape::ModelCall(Box::new(ModelCallShape {
            author: AuthorShape::of(&duo.critic),
            model: self.model.origin(),
            system: self.system.origin(),
            history,
            tools: Vec::new(),
            enabled: None,
            tool_concurrency: None,
            round_limit: None,
            critic: Some(CriticShape {
                max_critiques: duo.budget.clone(),
                final_generation: duo.final_generation,
                verdict_retries: self.retries.clone(),
                review_request: self.view.review_request.origin(),
                verdict_override: matches!(self.verdict, VerdictForm::Override(_)),
            }),
        })))
    }
}

impl CheckNode {
    /// `node`, holding what the check reads: the revision limit, the rule
    /// check when set and the empty-answer critique.
    pub(crate) fn description(&self) -> NodeDescription {
        NodeDescription::of(Shape::Check(CheckShape {
            revision_limit: self.duo.revision_limit.clone(),
            rule_check: self.rule_check.as_ref().map(|check| check.origin()),
            empty_answer_critique: self.empty_answer_critique.origin(),
        }))
    }
}
