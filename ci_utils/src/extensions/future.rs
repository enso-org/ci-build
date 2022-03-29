use crate::prelude::*;

pub trait FutureExt: Future {
    // fn spawn(self) -> tokio::task::JoinHandle<Self::Output>
    // where
    //     Self: Future + Send + 'static + Sized,
    //     Self::Output: Send + 'static, {
    //     tokio::spawn(self)
    // }
}

impl<T: ?Sized> FutureExt for T where T: Future {}
