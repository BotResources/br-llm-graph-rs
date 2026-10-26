use std::sync::Arc;

use br_graph_contract::{CancelView, Event, EventSender, Fields, InputReader, Reason, State};

use super::{LateInput, Rules};
use crate::signals::{Awaited, Signals, settle};

enum Decision {
    Wait,
    Fire,
    Refuse,
}

fn decide<F: Fields>(rules: &Rules<F>, late_input: LateInput, state: &State<F>) -> Decision {
    if (rules.spent)(state) {
        match late_input {
            LateInput::Silent => return Decision::Wait,
            LateInput::Fail => return Decision::Refuse,
            LateInput::FireAgain => {}
        }
    }
    if (rules.ready)(state) {
        Decision::Fire
    } else {
        Decision::Wait
    }
}

pub(super) async fn run<F: Fields>(
    rules: Arc<Rules<F>>,
    late_input: LateInput,
    mut reader: InputReader<F>,
    sender: EventSender<F>,
    cancel: CancelView,
) {
    let mut signals = Signals::new(&cancel, &sender);
    loop {
        match signals.next_input(&mut reader).await {
            Awaited::Gone => return,
            Awaited::Cancelled => return settle(&mut reader, sender, Event::Cancelled),
            Awaited::Input(input) => match decide(&rules, late_input, &input) {
                Decision::Wait => {}
                Decision::Fire => {
                    return settle(&mut reader, sender, Event::Result(F::empty_update()));
                }
                Decision::Refuse => {
                    let reason = Reason::new(format!(
                        "merger {}: the merge was already done, a late input is refused",
                        rules.name
                    ));
                    return settle(&mut reader, sender, Event::Refused(reason));
                }
            },
        }
    }
}
