use crate::prelude::*;
use serde::de::DeserializeOwned;

pub trait PathExt: AsRef<Path> {
    fn join_many<P: AsRef<Path>>(&self, segments: impl IntoIterator<Item = P>) -> PathBuf {
        let mut ret = self.as_ref().to_path_buf();
        ret.extend(segments);
        ret
    }

    /// Appends a new extension to the file.
    ///
    /// Does not try to replace previous extension, unlike `set_extension`.
    ///
    /// ```
    /// use ide_ci::extensions::path::PathExt;
    /// use std::path::PathBuf;
    ///
    /// let path = PathBuf::from("foo.tar").with_appended_extension("gz");
    /// assert_eq!(path, PathBuf::from("foo.tar.gz"));
    ///
    /// let path = PathBuf::from("foo").with_appended_extension("zip");
    /// assert_eq!(path, PathBuf::from("foo.zip"));
    /// ```
    fn with_appended_extension(&self, extension: impl AsRef<OsStr>) -> PathBuf {
        let mut ret = self.as_ref().to_path_buf().into_os_string();
        ret.push(".");
        ret.push(extension.as_ref());
        ret.into()
    }

    #[context("Failed to deserialize file `{}` as type `{}`.", self.as_ref().display(), std::any::type_name::<T>())]
    fn read_to_json<T: DeserializeOwned>(&self) -> Result<T> {
        let file = std::fs::File::open(self)?;
        serde_json::from_reader(file).anyhow_err()
    }

    fn write_as_json<T: Serialize>(&self, value: &T) -> Result {
        let file = std::fs::File::create(self)?;
        serde_json::to_writer(file, value).anyhow_err()
    }

    fn as_str(&self) -> &str {
        self.as_ref().to_str().unwrap()
    }
}

impl<T: AsRef<Path>> PathExt for T {}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
}
