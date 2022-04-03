use crate::prelude::*;

pub trait FutureExt: Future {
    // fn spawn(self) -> tokio::task::JoinHandle<Self::Output>
    // where
    //     Self: Future + Send + 'static + Sized,
    //     Self::Output: Send + 'static, {
    //     tokio::spawn(self)
    // }
    // fn map_err<E>(self) -> impl Future<Output=Result<Self::Output>>
    //     where
    //         Self: Sized,
    //         Self::Ou
    // {  }
}

impl<T: ?Sized> FutureExt for T where T: Future {}


pub fn receiver_to_stream<T>(
    mut receiver: tokio::sync::mpsc::Receiver<T>,
) -> impl Stream<Item = T> {
    futures::stream::poll_fn(move |ctx| receiver.poll_recv(ctx))
}
