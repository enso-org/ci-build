use crate::prelude::*;

pub fn display_sequence(
    sequence: impl IntoIterator<Item: ToString>,
    f: &mut std::fmt::Formatter,
) -> std::fmt::Result {
    f.debug_list().entries(sequence.into_iter().map(|item| item.to_string())).finish()
}
