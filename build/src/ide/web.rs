use crate::prelude::*;

use ide_ci::io::download_all;

pub const ARCHIVED_ASSET_FILE: &str = "ide-assets-main/content/assets/";

pub async fn download_js_assets(content_path: impl AsRef<Path>) -> Result {
    let output = content_path.as_ref().join("assets");
    let archived_asset_prefix = PathBuf::from(ARCHIVED_ASSET_FILE);
    let ide_assets_url = "https://github.com/enso-org/ide-assets/archive/refs/heads/main.zip";
    let archive = download_all(ide_assets_url).await?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive))?;
    ide_ci::archive::zip::extract_subtree(&mut archive, &archived_asset_prefix, &output)?;
    Ok(())
}

pub struct Inputs {
    pub assets: PathBuf,
    pub icons:  PathBuf,
    pub wasm:   PathBuf,
}
