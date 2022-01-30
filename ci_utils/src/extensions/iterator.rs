use crate::prelude::*;
use std::iter::Rev;
use std::iter::Take;

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

pub trait ExactDoubleEndedIteratorExt: ExactSizeIterator + DoubleEndedIterator + Sized {
    fn take_last_n(self, n: usize) -> Rev<Take<Rev<Self>>> {
        self.rev().take(n).rev()
    }
}

impl<T> ExactDoubleEndedIteratorExt for T where T: ExactSizeIterator + DoubleEndedIterator {}
