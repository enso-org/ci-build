use crate::prelude::*;
use std::fmt::Formatter;

use crate::version::default_engine_version;
use crate::version::Versions;
use platforms::TARGET_ARCH;
use platforms::TARGET_OS;

#[cfg(target_os = "linux")]
pub const LIBRARIES_TO_TEST: [&str; 6] =
    ["Tests", "Table_Tests", "Database_Tests", "Geo_Tests", "Visualization_Tests", "Image_Tests"];

// Test postgres only on Linux
#[cfg(not(target_os = "linux"))]
pub const LIBRARIES_TO_TEST: [&str; 5] =
    ["Tests", "Table_Tests", "Geo_Tests", "Visualization_Tests", "Image_Tests"];

pub const ARCHIVE_EXTENSION: &str = match TARGET_OS {
    OS::Windows => "zip",
    _ => "tar.gz",
};

#[derive(Clone, Debug)]
pub struct ComponentPaths {
    // e.g. `enso-engine-0.0.0-SNAPSHOT.2022-01-19-windows-amd64`
    pub name:             PathBuf,
    // e.g. H:\NBO\enso\built-distribution\enso-engine-0.0.0-SNAPSHOT.2022-01-19-windows-amd64
    pub root:             PathBuf,
    // e.g. H:\NBO\enso\built-distribution\enso-engine-0.0.0-SNAPSHOT.2022-01-19-windows-amd64\
    // enso-0.0.0-SNAPSHOT.2022-01-19
    pub dir:              PathBuf,
    // e.g. H:\NBO\enso\built-distribution\enso-engine-0.0.0-SNAPSHOT.2022-01-19-windows-amd64.zip
    pub artifact_archive: PathBuf,
}

impl ComponentPaths {
    pub fn new(
        build_root: &Path, // e.g. H:\NBO\enso\built-distribution
        name_prefix: &str,
        dirname: &str,
        triple: &TargetTriple,
    ) -> Self {
        let name = PathBuf::from(iformat!("{name_prefix}-{triple}"));
        let root = build_root.join(&name);
        let dir = root.join(dirname);
        let artifact_archive = root.with_appended_extension(ARCHIVE_EXTENSION);
        Self { name, root, dir, artifact_archive }
    }

    pub fn emit_to_actions(&self, prefix: &str) -> Result {
        let paths = [
            ("NAME", &self.name),
            ("ROOT", &self.root),
            ("DIR", &self.dir),
            ("ARCHIVE", &self.artifact_archive),
        ];
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
pub struct TargetTriple {
    pub os:       OS,
    pub arch:     Arch,
    pub versions: Versions,
}

impl TargetTriple {
    pub fn new(versions: Versions) -> Self {
        Self { os: TARGET_OS, arch: TARGET_ARCH, versions }
    }


    /// Pretty prints architecture for our packages. Conform to GraalVM scheme as well.
    pub fn arch(&self) -> &'static str {
        match self.arch {
            Arch::X86_64 => "amd64",
            Arch::AArch64 if self.os == OS::MacOS => {
                // No Graal packages for Apple Silicon.
                "amd64"
            }
            Arch::AArch64 => "aarch64",
            _ => panic!("Unrecognized architecture {}", self.arch),
        }
    }
}

impl Display for TargetTriple {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}-{}", self.versions.version, self.os, self.arch())
    }
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub repo_root:       PathBuf,
    pub build_dist_root: PathBuf,
    pub target:          PathBuf,
    pub launcher:        ComponentPaths,
    pub engine:          ComponentPaths,
    pub project_manager: ComponentPaths,
    pub triple:          TargetTriple,
    /* graal_dist_name: PathBuf,
     * graal_dist_root: PathBuf, */
}

impl Paths {
    pub fn new(repo_root: impl Into<PathBuf>) -> Result<Self> {
        let repo_root: PathBuf = repo_root.into().absolutize()?.into();
        let build_sbt = repo_root.join("build.sbt");
        let build_sbt_contents = std::fs::read_to_string(build_sbt)?;
        let version =
            crate::get_enso_version(&build_sbt_contents).unwrap_or(default_engine_version());
        Self::new_version(repo_root, version)
    }

    pub fn distribution(&self) -> PathBuf {
        self.repo_root.join("distribution")
    }

    /// Create a new set of paths for building the Enso with a given version number.
    pub fn new_version(repo_root: impl Into<PathBuf>, version: Version) -> Result<Self> {
        let repo_root: PathBuf = repo_root.into().absolutize()?.into();
        let build_dist_root = repo_root.join("built-distribution");
        let target = repo_root.join("target");

        let versions = Versions::new(version);

        let triple = TargetTriple::new(versions);
        let launcher = ComponentPaths::new(&build_dist_root, "enso-launcher", "enso", &triple);
        let engine = ComponentPaths::new(
            &build_dist_root,
            "enso-engine",
            &format!("enso-{}", &triple.versions.version),
            &triple,
        );
        let project_manager =
            ComponentPaths::new(&build_dist_root, "enso-project-manager", "enso", &triple);
        Ok(Paths { repo_root, build_dist_root, target, launcher, engine, project_manager, triple })
    }

    /// Sets the environment variables in the current process and in GitHub Actions Runner (if being
    /// run in its environment), so future steps of the job also have access to them.
    pub fn emit_env_to_actions(&self) -> Result {
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

    pub fn stdlib_tests(&self) -> PathBuf {
        self.repo_root.join("test")
    }

    pub fn stdlib_test(&self, test_name: impl AsRef<Path>) -> PathBuf {
        self.stdlib_tests().join(test_name)
    }

    pub fn changelog(&self) -> PathBuf {
        root_to_changelog(&self.repo_root)
    }

    pub fn edition_name(&self) -> String {
        self.triple.versions.edition_name()
    }

    // e.g. enso2\distribution\editions\2021.20-SNAPSHOT.yaml
    pub fn edition_file(&self) -> PathBuf {
        self.distribution()
            .join_many(["editions", &self.edition_name()])
            .with_appended_extension("yaml")
    }

    pub fn version(&self) -> &Version {
        &self.triple.versions.version
    }
}

pub fn root_to_changelog(root: impl AsRef<Path>) -> PathBuf {
    root.as_ref().join_many(["app", "gui", "CHANGELOG.md"])
}
