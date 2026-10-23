use std::fmt;

use crate::{Cause, Fields, Reason};

pub enum Event<F: Fields> {
    Result(F::Update),
    Failed(Cause),
    Refused(Reason),
    Cancelled,
}

impl<F: Fields> fmt::Debug for Event<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Result(_) => f.write_str("Result(..)"),
            Self::Failed(cause) => f.debug_tuple("Failed").field(cause).finish(),
            Self::Refused(reason) => f.debug_tuple("Refused").field(reason).finish(),
            Self::Cancelled => f.write_str("Cancelled"),
        }
    }
}
