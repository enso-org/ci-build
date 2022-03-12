//! Wrappers over [`std::fs`] functions that provide sensible error messages, i.e. explaining what
//! operation was attempted and what was the relevant path.
use crate::prelude::*;

use std::fs::File;

#[context("Failed to read the file: {}", path.as_ref().display())]
pub fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    std::fs::read(&path).anyhow_err()
}

#[context("Failed to read the file: {}", path.as_ref().display())]
pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    std::fs::read_to_string(&path).anyhow_err()
}

#[context("Failed to write path: {}", path.as_ref().display())]
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result {
    std::fs::write(&path, contents).anyhow_err()
}

#[context("Failed to open path for writing: {}", path.as_ref().display())]
pub fn create(path: impl AsRef<Path>) -> Result<File> {
    File::create(&path).anyhow_err()
}

#[context("Failed to create missing directories no path: {}", path.as_ref().display())]
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> Result {
    std::fs::create_dir_all(&path).anyhow_err()
}
