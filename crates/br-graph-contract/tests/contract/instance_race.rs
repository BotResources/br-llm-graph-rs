use std::sync::{Arc, Barrier};

use br_graph_contract::Instance;

use crate::common::id;
use crate::instance::input;

#[test]
fn given_a_race_of_gives_and_a_close_when_read_then_every_accepted_input_is_read_and_no_other() {
    for round in 0..300 {
        let (instance, mut reader, _sender) = Instance::<crate::common::Tally>::channel(id(round));
        let (input_end, _events) = instance.into_parts();
        let input_end = Arc::new(input_end);
        let start = Arc::new(Barrier::new(5));
        let givers: Vec<_> = (0..4)
            .map(|_| {
                let input_end = Arc::clone(&input_end);
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    (0..200)
                        .map(|total| input_end.give(input(total)).is_ok())
                        .collect::<Vec<bool>>()
                })
            })
            .collect();
        start.wait();
        reader.close();
        let per_giver: Vec<Vec<bool>> = givers
            .into_iter()
            .map(|giver| giver.join().unwrap())
            .collect();
        for outcomes in &per_giver {
            let first_refusal = outcomes.iter().position(|accepted| !accepted);
            if let Some(position) = first_refusal {
                assert!(outcomes[position..].iter().all(|accepted| !accepted));
            }
        }
        let accepted = per_giver
            .iter()
            .flatten()
            .filter(|accepted| **accepted)
            .count();
        assert_eq!(reader.drain().len(), accepted);
    }
}
