use crate::prelude::*;

use crate::io::create_dir_if_missing;

/// Archive formats that we handle.
#[derive(Copy, Clone, Debug)]
pub enum ArchiveFormat {
    TarGz,
    Zip,
}

impl ArchiveFormat {
    /// Deduce the archive format from a given filename.
    pub fn from_filename(filename: &Path) -> anyhow::Result<Self> {
        let extension = filename
            .extension()
            .ok_or_else(|| anyhow!("Cannot get extension of file {}", filename.display()))?;
        if extension == "zip" {
            Ok(ArchiveFormat::Zip)
        } else if extension == "gz" {
            let pre_extension =
                filename.file_stem().map(Path::new).and_then(|stem| stem.extension());
            if pre_extension.contains(&"tar") {
                Ok(ArchiveFormat::TarGz)
            } else {
                Err(anyhow!("Expecting tar archive to be compressed with GZ!"))
            }
        } else if extension == "tgz" {
            Ok(ArchiveFormat::TarGz)
        } else {
            Err(anyhow!("Cannot deduce archive format for file {}", filename.display()))
        }
    }

    /// Extract an archive of this format into a given output directory.
    pub fn extract(
        self,
        compressed_data: impl Read + Seek,
        output_dir: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        create_dir_if_missing(&output_dir)?;
        match self {
            ArchiveFormat::Zip => {
                let mut archive = zip::ZipArchive::new(compressed_data)?;
                archive.extract(output_dir)?;
            }
            ArchiveFormat::TarGz => {
                let tar_stream = flate2::read::GzDecoder::new(compressed_data);
                let mut archive = tar::Archive::new(tar_stream);
                archive.unpack(output_dir)?;
            }
        }
        Ok(())
    }
}
