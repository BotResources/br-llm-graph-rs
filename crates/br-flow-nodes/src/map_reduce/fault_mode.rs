#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaultMode {
    #[default]
    NonStrict,
    Strict,
}
