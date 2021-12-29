use crate::prelude::*;

pub trait TryIteratorExt: Iterator {
    type Ok;
    fn collect_result(self) -> Result<Vec<Self::Ok>>;
}

impl<T, U, E> TryIteratorExt for T
where
    T: Iterator<Item = std::result::Result<U, E>>,
    E: Into<anyhow::Error>,
{
    type Ok = U;
    fn collect_result(self) -> Result<Vec<U>> {
        self.map(|i| i.anyhow_err()).collect::<Result<Vec<_>>>()
    }
}
