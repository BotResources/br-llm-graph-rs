mod driver;
mod fields;
mod nodes;
mod panic_waker;
mod poisoned;
mod probe;

pub use driver::Driver;
pub use fields::{
    APPLY_PANIC, BAD_VALUE, EXPLODING_VALUE, Hook, Item, ItemUpdate, Report, ReportUpdate,
    VALIDATE_PANIC, both_filled, merged,
};
pub use nodes::{counting_fan_out, creation_at, fan_out, items_of, mode_at, nz, ordered, summed};
pub use poisoned::PoisonedFirst;
pub use probe::{CREATE_PANIC, CreateMode, Mark, Mode, Probe, ProbeCause};

use std::time::Duration;

pub const BOUND: Duration = Duration::from_secs(30);
pub const CREATION_GATE: u32 = u32::MAX;

pub async fn tasks_end() -> bool {
    for _ in 0..1000 {
        if tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks()
            == 0
        {
            return true;
        }
        tokio::task::yield_now().await;
    }
    false
}
