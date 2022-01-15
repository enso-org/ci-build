use crate::prelude::*;
use anyhow::Context;
use std::collections::BTreeSet;
use std::env::join_paths;
use std::env::set_var;
use std::env::split_paths;
use unicase::UniCase;

const PATH_ENVIRONMENT_NAME: &str = "PATH";

pub fn expect_var(name: impl AsRef<str>) -> Result<String> {
    let name = name.as_ref();
    std::env::var(name).context(anyhow!("Missing environment variable {}", name))
}

pub fn expect_var_os(name: impl AsRef<OsStr>) -> Result<OsString> {
    let name = name.as_ref();
    std::env::var_os(name)
        .ok_or_else(|| anyhow!("Missing environment variable {}", name.to_string_lossy()))
}

pub fn prepend_to_path(path: impl Into<PathBuf>) -> Result {
    let old_value = std::env::var_os(PATH_ENVIRONMENT_NAME);
    let old_pieces = old_value.iter().map(split_paths).flatten();
    let new_pieces = once(path.into()).chain(old_pieces);
    let new_value = join_paths(new_pieces)?;
    std::env::set_var(PATH_ENVIRONMENT_NAME, new_value);
    Ok(())
}

#[derive(Clone, Debug)]
pub enum Action {
    Remove,
    Set(String),
    PrependPaths(Vec<PathBuf>),
}

#[derive(Clone, Debug)]
pub struct Modification {
    pub variable_name: UniCase<String>,
    pub action:        Action,
}

impl Modification {
    pub fn apply(&self) -> Result {
        let normalized_name = &*self.variable_name;
        match &self.action {
            Action::Remove => {
                println!("Removing {}", self.variable_name);
                std::env::remove_var(normalized_name)
            }
            Action::Set(value) => {
                println!("Setting {}={}", self.variable_name, value);
                std::env::set_var(normalized_name, &value);
            }
            Action::PrependPaths(paths_to_prepend) =>
                if let Ok(old_value) = std::env::var(normalized_name) {
                    println!(
                        "Prepending to {} the following paths: {:?}",
                        self.variable_name, paths_to_prepend
                    );
                    let new_paths_set = paths_to_prepend.iter().collect::<BTreeSet<_>>();
                    let old_paths = split_paths(&old_value).collect_vec();

                    let old_paths_filtered =
                        old_paths.iter().filter(|old_path| !new_paths_set.contains(old_path));
                    let new_value = join_paths(paths_to_prepend.iter().chain(old_paths_filtered))?;
                    std::env::set_var(&*self.variable_name, new_value);
                } else {
                    let new_value = join_paths(paths_to_prepend)?;
                    set_var(&*self.variable_name, new_value);
                },
        };
        Ok(())
    }
}
