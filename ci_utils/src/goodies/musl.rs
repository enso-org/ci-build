use crate::prelude::*;

use crate::goodie::GoodieDatabase;

use lazy_static::lazy_static;
use platforms::TARGET_ARCH;
use platforms::TARGET_OS;
use std::env::consts::EXE_SUFFIX;

lazy_static! {
    pub static ref PROGRAM_NAME: String = format!("{}-musl-gcc{}", filename_stem(), EXE_SUFFIX);
}

pub struct Gcc;

impl Program for Gcc {
    fn executable_name() -> &'static str {
        &PROGRAM_NAME
    }
}

pub struct Musl;

pub struct Instance {
    directory: PathBuf,
}

impl crate::goodie::Instance for Instance {
    fn add_to_environment(&self) -> anyhow::Result<()> {
        std::env::set_var("TOOLCHAIN_DIR", &self.directory);
        crate::env::prepend_to_path(self.directory.join("bin"))
    }
}

#[async_trait]
impl Goodie for Musl {
    const NAME: &'static str = "musl libc toolchain";
    type Instance = Instance;

    async fn is_already_available(&self) -> Result<bool> {
        Ok(Gcc.lookup().is_ok())
    }

    async fn lookup(&self, database: &GoodieDatabase) -> Result<Self::Instance> {
        database.find_dir("musl").map(|directory| Instance { directory })
    }

    async fn install(&self, database: &GoodieDatabase) -> Result<Self::Instance> {
        // Reportedly for my "convenience". :(
        let archive_format = if TARGET_OS == OS::Windows { "zip" } else { "tgz" };
        let url = format!("https://musl.cc/{}.{}", filename_stem(), archive_format);
        let downloaded_dir = database.root_directory.join(filename_stem());
        let target_dir = database.root_directory.join("musl");
        crate::io::reset_dir(&downloaded_dir)?;
        crate::io::reset_dir(&target_dir)?;
        crate::io::download_and_extract(url.clone(), &database.root_directory).await?;
        std::fs::rename(downloaded_dir, target_dir)?;
        self.lookup(database).await
    }
}

pub fn target_path() -> String {
    let os_name = match TARGET_OS {
        OS::Linux => "linux",
        OS::Windows => "w64-mingw32",
        other_os => unimplemented!("System `{}` is not supported!", other_os),
    };

    let arch_name = match TARGET_ARCH {
        Arch::X86_64 => "x86_64",
        Arch::AArch64 => "aarch64",
        other_arch => unimplemented!("Architecture `{}` is not supported!", other_arch),
    };

    format!("{arch_name}-{os_name}")
}

pub fn filename_stem() -> String {
    format!("{}-native", target_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn musl_get_test() -> Result {
        let db = GoodieDatabase::new()?;
        db.require(&Musl).await?;
        Ok(())
    }
}
