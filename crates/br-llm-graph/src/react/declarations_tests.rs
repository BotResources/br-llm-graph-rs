//! The keys an agent's graph declares: one key of one kind declared once,
//! whoever gives it; a key of another kind, or a key the graph keeps for
//! itself, collides.

use crate::error::GraphError;
use crate::react::declarations::Declarations;
use crate::react::given::Reads;
use crate::react::test_support::{ckey, key, skey};
use crate::state::{Kind, Schema};

/// An agent's own keys: `opening` and `answer`, kept for itself, and the
/// configuration key `width` of a limit.
fn own() -> Declarations {
    let schema = Schema::builder()
        .state_by_name(key("opening"), Kind::Conversation)
        .state_by_name(key("answer"), Kind::Str)
        .config_by_name(key("width"), Kind::Int)
        .build();
    Declarations::new(schema, vec![key("opening"), key("answer")])
}

#[test]
fn given_keys_read_by_several_functions_when_declared_then_each_once_inputs_in_order() {
    let mut first = Reads::new();
    first.read_state(&skey::<String>("persona"));
    first.read_config(&ckey::<String>("tone"));
    let mut second = Reads::new();
    second.read_state(&skey::<Vec<String>>("notes"));
    second.read_state(&skey::<String>("persona"));
    second.read_config(&ckey::<String>("tone"));
    let mut declarations = own();

    declarations.reads(&first).unwrap();
    declarations.reads(&second).unwrap();

    let (schema, inputs) = declarations.finish();
    assert_eq!(inputs, vec![key("persona"), key("notes")]);
    assert_eq!(schema.state.get(&key("persona")), Some(&Kind::Str));
    assert_eq!(
        schema.state.get(&key("notes")),
        Some(&Kind::list(Kind::Str))
    );
    assert_eq!(schema.config.get(&key("tone")), Some(&Kind::Str));
    assert_eq!(schema.config.len(), 2);
}

#[test]
fn given_a_key_given_by_a_function_and_as_an_input_with_one_kind_when_declared_then_one_input() {
    let mut reads = Reads::new();
    reads.read_state(&skey::<Vec<String>>("offered"));
    let mut declarations = own();

    declarations
        .input(&key("offered"), Kind::list(Kind::Str))
        .unwrap();
    declarations.reads(&reads).unwrap();
    declarations
        .input(&key("offered"), Kind::list(Kind::Str))
        .unwrap();

    let (schema, inputs) = declarations.finish();
    assert_eq!(inputs, vec![key("offered")]);
    assert_eq!(
        schema.state.get(&key("offered")),
        Some(&Kind::list(Kind::Str))
    );
}

#[test]
fn given_a_function_reading_the_key_of_a_limit_with_its_kind_when_declared_then_one_key() {
    let mut reads = Reads::new();
    reads.read_config(&ckey::<i64>("width"));
    let mut declarations = own();

    declarations.reads(&reads).unwrap();

    let (schema, _) = declarations.finish();
    assert_eq!(schema.config.get(&key("width")), Some(&Kind::Int));
    assert_eq!(schema.config.len(), 1);
}

#[test]
fn given_a_key_the_graph_keeps_for_itself_when_declared_then_key_collision_whatever_its_kind() {
    for kind in [Kind::Str, Kind::Conversation] {
        for name in ["opening", "answer"] {
            assert!(matches!(
                own().input(&key(name), kind.clone()),
                Err(GraphError::KeyCollision { key: found }) if found == key(name)
            ));
        }
    }
}

#[test]
fn given_one_key_declared_with_two_kinds_when_declared_then_key_collision() {
    let mut first = Reads::new();
    first.read_state(&skey::<String>("persona"));
    let mut second = Reads::new();
    second.read_state(&skey::<Vec<String>>("persona"));
    let mut declarations = own();
    declarations.reads(&first).unwrap();
    assert!(matches!(
        declarations.reads(&second),
        Err(GraphError::KeyCollision { key: found }) if found == key("persona")
    ));

    let mut declarations = own();
    declarations.input(&key("scope"), Kind::Str).unwrap();
    assert!(matches!(
        declarations.input(&key("scope"), Kind::Int),
        Err(GraphError::KeyCollision { key: found }) if found == key("scope")
    ));

    let mut alone = Reads::new();
    alone.read_config(&ckey::<String>("width"));
    assert!(matches!(
        own().reads(&alone),
        Err(GraphError::KeyCollision { key: found }) if found == key("width")
    ));
}

#[test]
fn given_a_state_key_and_a_configuration_key_of_one_name_when_declared_then_both() {
    let mut reads = Reads::new();
    reads.read_state(&skey::<String>("tone"));
    reads.read_config(&ckey::<String>("tone"));
    let mut declarations = own();
    declarations.reads(&reads).unwrap();
    let (schema, inputs) = declarations.finish();
    assert_eq!(inputs, vec![key("tone")]);
    assert_eq!(schema.config.get(&key("tone")), Some(&Kind::Str));
}
