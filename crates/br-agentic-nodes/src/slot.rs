use br_llm_clients_core::BoxFuture;

use crate::client::{FallbackList, ModelClient};
use crate::fault::{AttemptError, Fault};

#[derive(Debug, Clone)]
pub struct Slot {
    list: FallbackList,
    settled: Option<usize>,
}

impl Slot {
    pub fn new(list: FallbackList) -> Self {
        Self {
            list,
            settled: None,
        }
    }

    pub fn is_settled(&self) -> bool {
        self.settled.is_some()
    }

    fn candidate(&self, tried: usize) -> Option<(usize, ModelClient)> {
        let index = match self.settled {
            Some(index) if tried == 0 => index,
            Some(_) => return None,
            None => tried,
        };
        self.list.get(index).cloned().map(|client| (index, client))
    }

    fn may_switch(&self, tried: usize) -> bool {
        self.settled.is_none() && tried + 1 < self.list.len()
    }
}

pub(crate) trait Attempt {
    type Output;

    fn attempt<'a>(
        &'a mut self,
        client: &'a ModelClient,
    ) -> BoxFuture<'a, Result<Self::Output, AttemptError>>;
}

pub(crate) async fn drive<A: Attempt>(
    slot: &mut Slot,
    attempt: &mut A,
) -> Result<A::Output, Fault> {
    let mut tried = 0;
    loop {
        let Some((index, client)) = slot.candidate(tried) else {
            return Err(Fault::Value("no client left for the model call".to_owned()));
        };
        match attempt.attempt(&client).await {
            Ok(output) => {
                slot.settled = Some(index);
                return Ok(output);
            }
            Err(AttemptError::Fatal(fault)) => return Err(fault),
            Err(AttemptError::Failed(failure)) => {
                if !slot.may_switch(tried) {
                    return Err(Fault::Model(failure));
                }
                tried += 1;
            }
        }
    }
}
