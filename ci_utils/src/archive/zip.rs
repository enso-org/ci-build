use crate::prelude::*;

pub use ::zip::*;
use anyhow::Context;

use zip::read::ZipFile;

pub fn extract_file(file: &mut ZipFile, output: impl AsRef<Path>) -> Result {
    println!("Extracting {}", output.as_ref().display());
    if file.is_dir() {
        crate::io::create_dir_if_missing(&output)?;
    } else {
        let mut output_file = crate::io::create(&output)?;
        std::io::copy(file, &mut output_file)?;
    }
    // Get and Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(mode) = file.unix_mode() {
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

pub fn extract_subtree(
    archive: &mut ZipArchive<impl Read + Seek>,
    prefix: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result {
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let path_in_archive = file
            .enclosed_name()
            .context(format!("Illegal path in the archive: {}", file.name()))?;
        if let Ok(relative_path) = path_in_archive.strip_prefix(&prefix) {
            let output = output.as_ref().join(relative_path);
            // let msg = format!("Extracting {}", output.display());
            // bar.set_message(msg);
            // std::thread::sleep(Duration::from_secs(1));
            extract_file(&mut file, output)?;
        }
    }
    Ok(())
}
