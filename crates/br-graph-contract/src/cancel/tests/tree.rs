use super::super::CancelToken;

const DEPTH: usize = 100_000;

#[test]
fn given_a_parent_when_triggered_then_children_and_grandchildren_are_triggered_at_once() {
    let parent = CancelToken::new();
    let child = parent.view().child();
    let grandchild = child.view().child();
    parent.handle().trigger();
    assert!(child.view().is_triggered());
    assert!(grandchild.view().is_triggered());
}

#[test]
fn given_a_child_when_triggered_then_the_parent_and_siblings_are_not() {
    let parent = CancelToken::new();
    let child = parent.child();
    let sibling = parent.child();
    child.handle().trigger();
    assert!(!parent.view().is_triggered());
    assert!(!sibling.view().is_triggered());
}

#[test]
fn given_a_triggered_parent_when_a_child_is_made_then_it_is_born_triggered() {
    let parent = CancelToken::new();
    parent.handle().trigger();
    assert!(parent.view().child().view().is_triggered());
}

#[test]
fn given_a_dropped_middle_token_when_the_root_is_triggered_then_the_grandchild_is_triggered() {
    let root = CancelToken::new();
    let grandchild = root.child().child();
    root.handle().trigger();
    assert!(grandchild.view().is_triggered());
}

#[test]
fn given_dropped_middle_views_when_the_root_is_triggered_then_the_grandchild_is_triggered() {
    let root = CancelToken::new();
    let grandchild = root.view().child().view().child();
    root.handle().trigger();
    assert!(grandchild.view().is_triggered());
}

#[test]
fn given_a_dropped_middle_token_when_the_middle_ancestor_is_triggered_then_the_grandchild_is_triggered()
 {
    let root = CancelToken::new();
    let middle = root.child();
    let middle_handle = middle.handle().clone();
    let grandchild = middle.child();
    drop(middle);
    middle_handle.trigger();
    assert!(grandchild.view().is_triggered());
    assert!(!root.view().is_triggered());
}

#[test]
fn given_a_very_deep_chain_when_the_root_is_triggered_then_the_leaf_is_triggered_without_recursion()
{
    let root = CancelToken::new();
    let mut leaf = root.child();
    for _ in 0..DEPTH {
        leaf = leaf.child();
    }
    root.handle().trigger();
    assert!(leaf.view().is_triggered());
}

#[test]
fn given_a_very_deep_chain_when_every_level_is_kept_then_every_level_is_triggered() {
    let root = CancelToken::new();
    let mut tokens = vec![root.child()];
    for _ in 0..DEPTH {
        let next = tokens.last().unwrap().child();
        tokens.push(next);
    }
    root.handle().trigger();
    assert!(tokens.iter().all(|token| token.view().is_triggered()));
}

#[test]
fn given_a_very_deep_chain_when_the_leaf_is_dropped_then_the_chain_is_released_without_recursion() {
    let root = CancelToken::new();
    let mut leaf = root.child();
    for _ in 0..DEPTH {
        leaf = leaf.child();
    }
    drop(leaf);
    assert_eq!(root.view().shared.live_child_count(), 0);
}

#[test]
fn given_a_very_deep_chain_when_the_root_is_dropped_first_then_dropping_the_leaf_is_safe() {
    let root = CancelToken::new();
    let mut leaf = root.child();
    for _ in 0..DEPTH {
        leaf = leaf.child();
    }
    drop(root);
    assert!(!leaf.view().is_triggered());
    drop(leaf);
}

#[test]
fn given_a_very_deep_chain_when_dropped_from_two_threads_then_it_is_released_once() {
    let root = CancelToken::new();
    let mut left = root.child();
    for _ in 0..DEPTH {
        left = left.child();
    }
    let right = left.child();
    let handle = std::thread::spawn(move || drop(right));
    drop(left);
    handle.join().unwrap();
    assert_eq!(root.view().shared.live_child_count(), 0);
}
