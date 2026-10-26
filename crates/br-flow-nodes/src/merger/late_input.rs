#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LateInput {
    #[default]
    Silent,
    Fail,
    FireAgain,
}
