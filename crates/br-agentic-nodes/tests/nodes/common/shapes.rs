use std::sync::Mutex;

use br_agentic_contract::{AgentLabel, PortError, PortFuture};
use br_agentic_nodes::OutputSchemaPort;
use br_llm_clients_core::{
    ClientRequest, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};

use super::Doc;

pub fn reply_shape() -> StructuredShape {
    StructuredShape::new(
        "reply",
        Some("The reply of the agent.".to_owned()),
        Parameters::new(vec![
            Parameter::new("answer", "The answer.", true, ParameterKind::String).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap()
}

pub fn asked_shape(request: &ClientRequest) -> Option<&StructuredShape> {
    match request.output() {
        OutputMode::Structured(shape) => Some(shape),
        OutputMode::Plain => None,
    }
}

pub struct Shapes {
    shape: Option<StructuredShape>,
    fail: bool,
    pub seen: Mutex<Vec<(String, String)>>,
}

impl Shapes {
    pub fn giving(shape: StructuredShape) -> Self {
        Self {
            shape: Some(shape),
            fail: false,
            seen: Mutex::new(Vec::new()),
        }
    }

    pub fn none() -> Self {
        Self {
            shape: None,
            fail: false,
            seen: Mutex::new(Vec::new()),
        }
    }

    pub fn failing() -> Self {
        Self {
            fail: true,
            ..Self::none()
        }
    }

    pub fn labels(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(_, label)| label.clone())
            .collect()
    }
}

impl OutputSchemaPort<Doc> for Shapes {
    fn shape<'a>(
        &'a self,
        snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<Option<StructuredShape>, PortError>> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push((snapshot.note.clone(), label.as_str().to_owned()));
            if self.fail {
                return Err(PortError::fault("the output-schema port failed"));
            }
            Ok(self.shape.clone())
        })
    }
}
