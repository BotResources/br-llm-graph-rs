#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]

#[path = "../nodes/common/mod.rs"]
mod common;

mod answer_failed;
mod answer_shape;
mod answer_stored;
mod answer_unwrap;
mod failure;
mod mails;
mod pair;
mod ports;
mod prelude;
mod prog_counter;
mod prog_limits;
mod prog_position;
mod prog_refuse;
mod prog_restored;
mod prog_when;
mod projection;
mod raw;
mod request;
mod seen;
mod thinking;
mod timeout;
mod wrap;
