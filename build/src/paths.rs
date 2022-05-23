use crate::prelude::*;
use std::env::consts::EXE_EXTENSION;
use std::fmt::Formatter;

use crate::version::Versions;

pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/paths.rs"));
}

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

#[derive(Clone, PartialEq, Debug, Default)]
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
        let name = PathBuf::from(iformat!("{name_prefix}-{triple.engine()}"));
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
                &path.to_string_lossy(),
            )?;
        }
        Ok(())
    }
}

pub fn pretty_print_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X86_64 => "amd64",
        Arch::AArch64 => "aarch64",
        _ => panic!("Unrecognized architecture {}", arch),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TargetTriple {
    pub os:       OS,
    pub arch:     Arch,
    pub versions: Versions,
}

impl TargetTriple {
    /// Create a new triple with OS and architecture are inferred from the hosting system.
    pub fn new(versions: Versions) -> Self {
        Self { os: TARGET_OS, arch: TARGET_ARCH, versions }
    }

    /// Get the triple effectively used by the Engine build.
    /// 
    /// As the GraalVM we use does not support native Aarch64 builds, it should be treated as amd64 there.
    pub fn engine(&self) -> Self {
        let mut ret = self.clone();
        ret.arch = 
        if self.arch == Arch::AArch64 && self.os == OS::MacOS {
            Arch::X86_64
        } else {
            self.arch
        };
        ret
    }

    /// Pretty prints architecture for our packages. Conform to GraalVM scheme as well.
    pub fn arch(&self) -> &'static str {
        pretty_print_arch(self.arch)
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
}

impl Paths {
    pub fn distribution(&self) -> PathBuf {
        self.repo_root.join("distribution")
    }

    /// Create a new set of paths for building the Enso with a given version number.
    pub fn new_triple(repo_root: impl Into<PathBuf>, triple: TargetTriple) -> Result<Self> {
        let repo_root: PathBuf = repo_root.into().absolutize()?.into();
        let build_dist_root = repo_root.join("built-distribution");
        let target = repo_root.join("target");
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

    /// Create a new set of paths for building the Enso with a given version number.
    pub fn new_versions(repo_root: impl Into<PathBuf>, versions: Versions) -> Result<Self> {
        let triple = TargetTriple::new(versions);
        Self::new_triple(repo_root, triple)
    }

    /// Create a new set of paths for building the Enso with a given version number.
    pub fn new_version(repo_root: impl Into<PathBuf>, version: Version) -> Result<Self> {
        let versions = Versions::new(version);
        Self::new_versions(repo_root, versions)
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

        ide_ci::actions::workflow::set_env("TARGET_DIR", &self.target.to_string_lossy())?;
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
            .join_iter(["editions", &self.edition_name()])
            .with_appended_extension("yaml")
    }

    pub async fn upload_edition_file_artifact(&self) -> Result {
        ide_ci::actions::artifacts::upload_single_file(self.edition_file(), "Edition File").await
    }

    pub async fn download_edition_file_artifact(&self) -> Result {
        ide_ci::actions::artifacts::download_single_file_artifact(
            "Edition File",
            self.edition_file(),
        )
        .await
    }

    pub fn version(&self) -> &Version {
        &self.triple.versions.version
    }

    pub fn build_sbt(&self) -> PathBuf {
        self.repo_root.join("build.sbt")
    }
}

pub fn root_to_changelog(root: impl AsRef<Path>) -> PathBuf {
    let changelog_filename = "CHANGELOG.md";
    let root_path = root.as_ref().join(changelog_filename);
    // TODO: transitional code to support both locations of the changelog
    //       only the root one should prevail
    if root_path.exists() {
        root_path
    } else {
        root.as_ref().join_iter(["app", "gui", changelog_filename])
    }
}

/// The default value of `ENSO_DATA_DIRECTORY`.
/// See: <https://enso.org/docs/developer/enso/distribution/distribution.html#installed-enso-distribution-layout>
pub fn default_data_directory() -> PathBuf {
    let project_path = match TARGET_OS {
        OS::MacOS => "org.enso",
        _ => "enso",
    };
    // We can unwrap, because all systems we target define data local directory.
    dirs::data_local_dir().unwrap().join(project_path)
}

/// Get the `ENSO_DATA_DIRECTORY` path.
pub fn data_directory() -> PathBuf {
    std::env::var_os("ENSO_DATA_DIRECTORY").map_or_else(|| default_data_directory(), PathBuf::from)
}

/// Get the place where global IR caches are stored.
pub fn cache_directory() -> PathBuf {
    data_directory().join("cache")
}

pub trait GuiPaths {
    fn root(&self) -> &Path;
    fn temp(&self) -> &Path;


    fn github(&self) -> PathBuf {
        self.root().join(".github")
    }
    fn github_workflows(&self) -> PathBuf {
        self.github().join("workflows")
    }

    fn dist(&self) -> PathBuf {
        self.root().join("dist")
    }

    fn dist_client(&self) -> PathBuf {
        self.dist().join("client")
    }

    fn dist_content(&self) -> PathBuf {
        self.dist().join("content")
    }

    fn dist_assets(&self) -> PathBuf {
        self.dist_content().join("assets")
    }

    fn dist_package_json(&self) -> PathBuf {
        self.dist_content().join("package.json")
    }

    fn dist_preload_js(&self) -> PathBuf {
        self.dist_content().join("preload.js")
    }

    fn dist_bin(&self) -> PathBuf {
        self.dist().join("bin")
    }

    fn dist_init(&self) -> PathBuf {
        self.dist().join("init")
    }

    fn dist_build_init(&self) -> PathBuf {
        self.dist().join("build-init")
    }

    fn dist_build_info(&self) -> PathBuf {
        self.dist().join("build.json")
    }

    fn dist_tmp(&self) -> PathBuf {
        self.dist().join("tmp")
    }

    const WASM_MAIN: &'static str = "ide.wasm";
    const WASM_MAIN_RAW: &'static str = "ide_bg.wasm";
    const WASM_GLUE: &'static str = "ide.js";

    // Final WASM artifacts in `dist` directory.
    fn dist_wasm(&self) -> PathBuf {
        self.dist().join("wasm")
    }

    fn dist_wasm_main(&self) -> PathBuf {
        self.dist().join(Self::WASM_MAIN)
    }

    fn dist_wasm_main_raw(&self) -> PathBuf {
        self.dist().join(Self::WASM_MAIN_RAW)
    }

    fn dist_wasm_glue(&self) -> PathBuf {
        self.dist().join(Self::WASM_GLUE)
    }

    // Intermediate WASM artifacts.
    fn wasm(&self) -> PathBuf {
        self.temp().join("enso-wasm")
    }

    fn wasm_main(&self) -> PathBuf {
        self.wasm().join(Self::WASM_MAIN)
    }

    fn wasm_main_raw(&self) -> PathBuf {
        self.wasm().join(Self::WASM_MAIN_RAW)
    }

    fn wasm_glue(&self) -> PathBuf {
        self.wasm().join(Self::WASM_GLUE)
    }

    fn wasm_main_gz(&self) -> PathBuf {
        self.wasm().join("ide.wasm.gz")
    }

    fn ide_desktop(&self) -> PathBuf {
        self.root().join_iter(["app", "ide-desktop"])
    }

    fn ide_desktop_lib_project_manager(&self) -> PathBuf {
        self.ide_desktop().join_iter(["lib", "project-manager"])
    }

    fn ide_desktop_lib_content(&self) -> PathBuf {
        self.ide_desktop().join_iter(["lib", "content"])
    }

    fn gui(&self) -> PathBuf {
        self.root().join_iter(["app", "gui"])
    }

    fn script(&self) -> PathBuf {
        self.root().join("build")
    }
}

pub fn project_manager(base_path: impl AsRef<Path>) -> PathBuf {
    base_path
        .as_ref()
        .join_iter(["enso", "bin", "project-manager"])
        .with_appended_extension(EXE_EXTENSION)
}
