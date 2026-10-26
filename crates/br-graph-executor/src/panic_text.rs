use std::any::Any;
use std::error::Error;
use std::mem;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn text_of(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "panic without a text message".to_owned()
}

pub(crate) fn panic_reason(payload: Box<dyn Any + Send>) -> String {
    let text = text_of(&*payload);
    release(payload);
    text
}

pub(crate) fn release(payload: Box<dyn Any + Send>) {
    if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
        mem::forget(second);
    }
}

pub(crate) fn display_of(cause: &(dyn Error + Send + Sync + 'static)) -> String {
    catch_unwind(AssertUnwindSafe(|| cause.to_string())).unwrap_or_else(panic_reason)
}
