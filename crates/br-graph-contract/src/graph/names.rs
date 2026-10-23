const NODE_NAME_MAX: usize = 64;
const NODE_KIND_MAX: usize = 32;
const NODE_SUBKIND_MAX: usize = 64;
const EDGE_NAME_MAX: usize = 64;
const VERSION_MAX: usize = 64;

fn is_identifier(value: &str, max: usize) -> bool {
    let bytes = value.as_bytes();
    bytes.len() <= max
        && bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

pub(super) fn is_node_name(value: &str) -> bool {
    is_identifier(value, NODE_NAME_MAX)
}

pub(super) fn is_node_kind(value: &str) -> bool {
    is_identifier(value, NODE_KIND_MAX)
}

pub(super) fn is_node_subkind(value: &str) -> bool {
    is_identifier(value, NODE_SUBKIND_MAX)
}

pub(super) fn is_edge_name(value: &str) -> bool {
    is_identifier(value, EDGE_NAME_MAX)
}

pub(super) fn is_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=VERSION_MAX).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
