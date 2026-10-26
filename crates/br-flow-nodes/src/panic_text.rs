use std::any::Any;
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

pub(crate) fn panic_text(payload: Box<dyn Any + Send>) -> String {
    let text = text_of(&*payload);
    release(payload);
    text
}

fn release(payload: Box<dyn Any + Send>) {
    if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
        mem::forget(second);
    }
}

pub(crate) fn caught<T>(run: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(run)).map_err(panic_text)
}
