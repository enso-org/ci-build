use crate::prelude::*;

use platforms::TARGET_ARCH;
use platforms::TARGET_OS;

#[derive(Clone, Debug)]
pub struct DistPaths {
    pub name: PathBuf,
    pub root: PathBuf,
    pub dir:  PathBuf,
}

impl DistPaths {
    pub fn new(build_root: &Path, name_prefix: &str, dirname: &str, triple: &str) -> Self {
        let name = PathBuf::from(iformat!("{name_prefix}-{triple}"));
        let root = build_root.join(&name);
        let dir = root.join(dirname);
        Self { name, root, dir }
    }

    pub fn emit_to_actions(&self, prefix: &str) -> Result {
        let paths = [("NAME", &self.name), ("ROOT", &self.root), ("DIR", &self.dir)];
        for (what, path) in paths {
            ide_ci::actions::workflow::set_env(
                &iformat!("{prefix}_DIST_{what}"),
                path.to_string_lossy(),
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub repo_root:       PathBuf,
    pub build_dist_root: PathBuf,
    pub target:          PathBuf,
    pub launcher:        DistPaths,
    pub engine:          DistPaths,
    pub project_manager: DistPaths,
    pub version:         Version,
    /* graal_dist_name: PathBuf,
     * graal_dist_root: PathBuf, */
}

impl Paths {
    pub fn new(repo_root: impl Into<PathBuf>) -> Result<Self> {
        let repo_root: PathBuf = repo_root.into().absolutize()?.into();
        let build_sbt = repo_root.join("build.sbt");
        let build_sbt_contents = std::fs::read_to_string(build_sbt)?;
        let version = crate::get_enso_version(&build_sbt_contents)?;
        let build_dist_root = repo_root.join("built-distribution");
        let target = repo_root.join("target");
        let arch = match TARGET_ARCH {
            Arch::X86_64 => "amd64",
            Arch::AArch64 if TARGET_OS == OS::MacOS => "amd64", /* No Graal packages for Apple */
            // Silicon.
            Arch::AArch64 => "aarch64",
            _ => panic!("Unrecognized architecture {}", TARGET_ARCH),
        };
        let triple = format!("{}-{}-{}", version, TARGET_OS, arch);
        let launcher = DistPaths::new(&build_dist_root, "enso-launcher", "enso", &triple);
        let engine =
            DistPaths::new(&build_dist_root, "enso-engine", &format!("enso-{}", version), &triple);
        let project_manager =
            DistPaths::new(&build_dist_root, "enso-project-manager", "enso", &triple);
        Ok(Paths { repo_root, build_dist_root, target, launcher, engine, project_manager, version })
    }

    pub fn emit_to_actions(&self) -> Result {
        let components = [
            ("ENGINE", &self.engine),
            ("LAUNCHER", &self.launcher),
            ("PROJECTMANAGER", &self.project_manager),
        ];

        for (prefix, paths) in components {
            paths.emit_to_actions(prefix)?;
        }

        ide_ci::actions::workflow::set_env("TARGET_DIR", self.target.to_string_lossy())?;
        Ok(())
    }
}
