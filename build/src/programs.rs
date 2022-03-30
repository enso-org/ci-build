use crate::prelude::*;

pub mod project_manager {
    use super::*;

    use ide_ci::program::command::MyCommand;

    pub struct ProjectManager;

    impl Program for ProjectManager {
        fn executable_name() -> &'static str {
            "project-manager"
        }
    }

    #[derive(Shrinkwrap)]
    #[shrinkwrap(mutable)]
    pub struct Command(pub ide_ci::program::Command);

    impl From<ide_ci::program::Command> for Command {
        fn from(inner: ide_ci::prelude::Command) -> Self {
            Self(inner)
        }
    }

    impl<P: Program> MyCommand<P> for Command {}
}
