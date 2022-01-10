use crate::prelude::*;

use crate::programs::Program;

pub struct Go;

impl Program for Go {
    fn executable_name() -> &'static str {
        "go"
    }
    fn default_locations(&self) -> Vec<PathBuf> {
        std::env::vars()
            .filter(|(name, _)| name.starts_with("GOROOT_"))
            .max_by(|(name1, _), (name2, _)| name1.cmp(name2))
            .map(|(_, value)| PathBuf::from(value).join("bin"))
            .into_iter()
            .collect()
    }

    fn version_command(&self) -> Result<Command> {
        let mut cmd = self.cmd()?;
        cmd.arg("version");
        Ok(cmd)
    }
}
