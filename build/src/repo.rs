use crate::prelude::*;

pub fn looks_like_enso_repository_root(path: impl AsRef<Path>) -> bool {
    (move || -> Result<bool> {
        let cargo_toml = path.as_ref().join("Cargo.toml");
        if !ide_ci::fs::read_to_string(cargo_toml)?.contains("[workspace]") {
            return Ok(false);
        }

        Ok(path.as_ref().join("build.sbt").exists())
    })()
    .unwrap_or(false)
}

pub fn deduce_repository_path() -> Option<PathBuf> {
    // let current_dir = std::env::current_dir().ok()?;
    let candidate_paths = [
        std::env::current_dir().ok(),
        std::env::current_dir().ok().and_then(|p| p.parent().map(ToOwned::to_owned)),
        std::env::current_dir().ok().and_then(|p| p.parent().map(|p| p.join("enso5"))),
        std::env::current_dir().ok().and_then(|p| p.parent().map(|p| p.join("enso"))),
    ];
    for candidate in candidate_paths {
        if let Some(path) = candidate && looks_like_enso_repository_root(&path) {
            println!("Deduced repository path to be {}.", path.display());
            return Some(path)
        }
    }
    println!("Failed to deduce the repository path.");
    None
}
