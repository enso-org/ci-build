use crate::prelude::*;
use anyhow::Error;

pub trait ResultExt<T, E> {
    fn anyhow_err(self) -> Result<T>;
}

impl<T, E> ResultExt<T, E> for std::result::Result<T, E>
where E: Into<Error>
{
    fn anyhow_err(self) -> Result<T> {
        self.map_err(E::into)
    }
}
