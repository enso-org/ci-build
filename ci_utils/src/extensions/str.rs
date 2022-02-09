use crate::prelude::*;

pub trait StrLikeExt {
    // FIXME: this needs better name!
    fn parse2<T: FromString>(&self) -> Result<T>;
}

impl<T: AsRef<str>> StrLikeExt for T {
    fn parse2<U: FromString>(&self) -> Result<U> {
        U::from_str(self.as_ref())
    }
}
