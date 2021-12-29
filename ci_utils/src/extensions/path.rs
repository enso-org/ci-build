use crate::prelude::*;

pub trait PathExt: AsRef<Path> {
    fn join_many<P: AsRef<Path>>(&self, segments: impl IntoIterator<Item = P>) -> PathBuf {
        let mut ret = self.as_ref().to_path_buf();
        ret.extend(segments);
        ret
    }

    fn append_extension(&self, extension: impl AsRef<OsStr>) -> PathBuf {
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

    #[test]
    fn append_extension() {
        assert_eq!(PathBuf::from("foo").append_extension("zip"), PathBuf::from("foo.zip"));
    }
}
