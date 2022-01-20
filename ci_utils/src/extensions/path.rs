use crate::prelude::*;

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
}

impl<T: AsRef<Path>> PathExt for T {}

#[cfg(test)]
mod tests {
    use super::*;
}
