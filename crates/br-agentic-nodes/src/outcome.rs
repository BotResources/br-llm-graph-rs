use br_llm_messages::ModelResponse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgement {
    Validated,
    Rejected,
    NotJudged,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReactOutcome {
    response: ModelResponse,
    steps: u32,
    step_limit_reached: bool,
}

impl ReactOutcome {
    pub(crate) fn new(response: ModelResponse, steps: u32, step_limit_reached: bool) -> Self {
        Self {
            response,
            steps,
            step_limit_reached,
        }
    }

    pub fn response(&self) -> &ModelResponse {
        &self.response
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn step_limit_reached(&self) -> bool {
        self.step_limit_reached
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratorCriticOutcome {
    response: ModelResponse,
    judgement: Judgement,
    iterations: u32,
    step_limit_reached: bool,
}

impl GeneratorCriticOutcome {
    pub(crate) fn new(
        response: ModelResponse,
        judgement: Judgement,
        iterations: u32,
        step_limit_reached: bool,
    ) -> Self {
        Self {
            response,
            judgement,
            iterations,
            step_limit_reached,
        }
    }

    pub fn response(&self) -> &ModelResponse {
        &self.response
    }

    pub fn judgement(&self) -> Judgement {
        self.judgement
    }

    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    pub fn step_limit_reached(&self) -> bool {
        self.step_limit_reached
    }
}
