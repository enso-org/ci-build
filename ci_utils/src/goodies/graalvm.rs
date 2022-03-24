use crate::prelude::*;

use crate::extensions::path::PathExt;
use crate::goodie::GoodieDatabase;
use crate::models::config::RepoContext;

use crate::programs::java;
use platforms::TARGET_OS;

#[derive(Clone, Copy, Debug, Default)]
pub struct Gu;

impl Program for Gu {
    fn executable_name() -> &'static str {
        "gu"
    }
}

#[derive(Clone, Debug)]
pub struct Instance {
    // Directory with extracted GraalVM
    pub path: PathBuf,
}

impl crate::goodie::Instance for Instance {
    fn add_to_environment(&self) -> anyhow::Result<()> {
        let root = match TARGET_OS {
            OS::MacOS => self.path.join_many(["Contents", "Home"]),
            _ => self.path.clone(),
        };

        std::env::set_var("JAVA_HOME", &root);
        std::env::set_var("GRAALVM_HOME", &root);
        crate::env::prepend_to_path(root.join("bin"))?;
        Ok(())
    }
}

/// Support for sulong has not been implemented for Windows yet.
///
/// See: https://github.com/oracle/graal/issues/1160
pub fn sulong_supported() -> bool {
    TARGET_OS != OS::Windows
}


const PACKAGE_PREFIX: &str = "graalvm-ce";

pub struct GraalVM<'a> {
    pub client:        &'a Octocrab,
    pub graal_version: Version,
    pub java_version:  java::LanguageVersion,
    pub os:            OS,
    pub arch:          Arch,
}

impl<'a> GraalVM<'a> {
    async fn find_graal_version() -> Result<Version> {
        let text = crate::programs::Java.version_string().await?;
        let line = text.lines().find(|line| line.contains("GraalVM")).ok_or_else(|| {
            anyhow!(
                "There is a Java environment available but it is not recognizable as GraalVM one,"
            )
        })?;
        crate::program::version::find_in_text(line)
    }

    async fn url(&self) -> anyhow::Result<Url> {
        let Self { graal_version, java_version, client, arch, os } = &self;

        let os_name = match *os {
            OS::Linux => "linux",
            OS::Windows => "windows",
            OS::MacOS => "darwin",
            other_os => unimplemented!("System `{}` is not supported!", other_os),
        };

        let arch_name = match *arch {
            Arch::X86_64 => "amd64",
            Arch::AArch64 if TARGET_OS == OS::MacOS => "amd64", /* No Graal packages for Apple */
            // Silicon.
            Arch::AArch64 => "aarch64",
            other_arch => unimplemented!("Architecture `{}` is not supported!", other_arch),
        };

        let java_version = format!("java{}", java_version.0);

        let platform_string =
            format!("{}-{}-{}-{}", PACKAGE_PREFIX, java_version, os_name, arch_name);
        let repo = RepoContext { owner: "graalvm".into(), name: "graalvm-ce-builds".into() };
        let release = repo.find_release_by_text(client, &graal_version.to_string()).await?;
        crate::github::find_asset_url_by_text(&release, &platform_string).cloned()
    }
}

#[async_trait]
impl<'a> Goodie for GraalVM<'a> {
    const NAME: &'static str = "GraalVM";
    type Instance = Instance;

    async fn is_already_available(&self) -> Result<bool> {
        Ok(Self::find_graal_version().await.contains(&self.graal_version))
    }

    async fn lookup(&self, database: &GoodieDatabase) -> Result<Self::Instance> {
        let expected_dir_name = PathBuf::from(format!(
            "{}-{}-{}",
            PACKAGE_PREFIX, self.java_version, self.graal_version
        ));
        for entry in database.root_directory.read_dir()? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.path().file_name().contains(&expected_dir_name)
            {
                return Ok(Instance { path: entry.path() });
            }
        }
        Err(anyhow!("no directory by name {} in the database.", expected_dir_name.display()))
    }

    async fn install(&self, database: &GoodieDatabase) -> Result<Self::Instance> {
        let graal_url = self.url().await?;
        crate::io::download_and_extract(graal_url.clone(), &database.root_directory).await?;
        self.lookup(database).await
    }
}
