use crate::prelude::*;

#[derive(Copy, Clone, Debug)]
pub struct Sbt;

impl Program for Sbt {
    fn executable_name() -> &'static str {
        "sbt"
    }
}

impl Sbt {
    /// Format a string with a command that will execute all the given tasks concurrently.
    pub fn concurrent_tasks(tasks: impl IntoIterator<Item: AsRef<str>>) -> String {
        let mut ret = String::from("all");
        for task in tasks {
            ret.push(' ');
            ret.push_str(task.as_ref())
        }
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_concurrent_tasks() {
        let tasks = ["test", "syntaxJS/fullOptJS"];
        assert_eq!(Sbt::concurrent_tasks(tasks), "all test syntaxJS/fullOptJS");
    }
}
