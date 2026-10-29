pub const WARNING_WINDOW: u32 = 10;

pub fn warning_text(remaining: u32) -> Option<String> {
    match remaining {
        0 => None,
        1 => Some(
            "You have one step left. Give your final answer now and ask for no tool call."
                .to_owned(),
        ),
        left if left <= WARNING_WINDOW => Some(format!(
            "You have {left} steps left to finish. Finish your work and give your final answer before the steps run out."
        )),
        _ => None,
    }
}
