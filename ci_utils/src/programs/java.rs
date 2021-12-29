use crate::prelude::*;

pub struct Java;

impl Program for Java {
    fn executable_name() -> &'static str {
        "java"
    }
}
