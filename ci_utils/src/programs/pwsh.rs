use crate::prelude::*;

pub struct PwSh;

impl Program for PwSh {
    fn executable_name(&self) -> &'static str {
        "pwsh"
    }
    fn executable_name_fallback() -> Vec<&'static str> {
        vec!["powershell"]
    }
}
