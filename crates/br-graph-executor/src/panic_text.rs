use std::any::Any;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub(crate) fn panic_reason(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "panic without a text message".to_owned()
}

pub(crate) fn display_of(cause: &(dyn Error + Send + Sync + 'static)) -> String {
    catch_unwind(AssertUnwindSafe(|| cause.to_string()))
        .unwrap_or_else(|payload| panic_reason(&*payload))
}
