use crate::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct Lerna;

impl Program for Lerna {
    fn executable_name() -> &'static str {
        "lerna"
    }
}
