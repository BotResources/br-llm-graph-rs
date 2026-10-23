use std::future::Future;
use std::pin::Pin;

use crate::{CancelView, Cause, Fields, Instance, InstanceId};

pub const PROGRAMMATIC: &str = "programmatic";

#[derive(Debug, thiserror::Error)]
#[error("the node could not create an instance")]
pub struct CreateError {
    #[source]
    cause: Cause,
}

impl CreateError {
    pub fn new(cause: impl Into<Cause>) -> Self {
        Self {
            cause: cause.into(),
        }
    }

    pub fn cause(&self) -> &Cause {
        &self.cause
    }

    pub fn into_cause(self) -> Cause {
        self.cause
    }
}

pub type CreateFuture<'a, F> =
    Pin<Box<dyn Future<Output = Result<Instance<F>, CreateError>> + Send + 'a>>;

pub trait Node<F: Fields>: Send + Sync {
    fn name(&self) -> &str;

    fn kind(&self) -> &str;

    fn subkind(&self) -> &str;

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, F>;
}
