use crate::prelude::*;

pub struct Flatc;

impl Program for Flatc {
    fn executable_name() -> &'static str {
        "flatc"
    }
}
