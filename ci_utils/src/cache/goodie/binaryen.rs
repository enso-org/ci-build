use crate::prelude::*;

use crate::cache;
use crate::env::prepend_to_path;

#[derive(Clone, Copy, Debug)]
pub struct Binaryen {
    pub version: usize,
}

impl Binaryen {}

impl cache::Goodie for Binaryen {
    fn url(&self) -> Result<Url> {
        let version = format!("version_{}", self.version);
        let target = match (TARGET_OS, TARGET_ARCH) {
            (OS::Windows, Arch::X86_64) => "x86_64-windows",
            (OS::Linux, Arch::X86_64) => "x86_64-linux",
            (OS::MacOS, Arch::X86_64) => "x86_64-macos",
            (OS::MacOS, Arch::AArch64) => "arm64-macos",
            (os, arch) => bail!("Not supported arch/OS combination: {arch}-{os}."),
        };
        let url =  format!("https://github.com/WebAssembly/binaryen/releases/download/{version}/binaryen-{version}-{target}.tar.gz");
        url.parse2()
    }

    fn enable(&self, package_path: PathBuf) -> Result {
        let bin_dir = package_path.join(format!("binaryen-version_{}", self.version)).join("bin");
        crate::fs::expect_dir(&bin_dir)?;
        prepend_to_path(bin_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache;
    use crate::log::setup_logging;
    use crate::programs;
    use crate::programs::wasm_opt::WasmOpt;

    #[tokio::test]
    async fn install_wasm_opt() -> Result {
        setup_logging()?;
        let cache = cache::Cache::new_default().await?;
        let binaryen = Binaryen { version: 108 };
        binaryen.install_if_missing(&cache, WasmOpt).await?;
        dbg!(programs::wasm_opt::WasmOpt.lookup())?;


        Ok(())
    }
}
