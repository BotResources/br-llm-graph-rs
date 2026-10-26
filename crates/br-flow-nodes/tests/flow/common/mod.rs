mod driver;
mod fields;
mod nodes;
mod probe;

pub use driver::Driver;
pub use fields::{BAD_VALUE, Item, ItemUpdate, Report, ReportUpdate, both_filled, merged};
pub use nodes::{counting_fan_out, fan_out, in_order, items_of, nz, summed};
pub use probe::{CreateMode, Mode, Probe, ProbeCause};

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
