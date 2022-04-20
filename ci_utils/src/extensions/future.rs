use crate::prelude::*;

use futures_util::future;
use futures_util::future::Map;
use futures_util::future::MapOk;
use futures_util::stream;
use futures_util::FutureExt as _;
use futures_util::TryFutureExt as _;

fn void<T>(_t: T) {}

pub trait FutureExt: Future {
    fn void(self) -> Map<Self, fn(Self::Output) -> ()>
    where Self: Sized {
        self.map(void)
    }
}

impl<T: ?Sized> FutureExt for T where T: Future {}

pub trait TryFutureExt: TryFuture {
    fn void_ok(self) -> MapOk<Self, fn(Self::Ok) -> ()>
    where Self: Sized {
        self.map_ok(void)
    }

    fn anyhow_err(self) -> future::MapErr<Self, fn(Self::Error) -> anyhow::Error>
    where
        Self: Sized,
        // TODO: we should rely on `into` rather than `from`
        anyhow::Error: From<Self::Error>, {
        self.map_err(anyhow::Error::from)
    }
}

impl<T: ?Sized> TryFutureExt for T where T: TryFuture {}



pub fn receiver_to_stream<T>(
    mut receiver: tokio::sync::mpsc::Receiver<T>,
) -> impl Stream<Item = T> {
    futures::stream::poll_fn(move |ctx| receiver.poll_recv(ctx))
}



pub trait TryStreamExt: TryStream {
    fn anyhow_err(self) -> stream::MapErr<Self, fn(Self::Error) -> anyhow::Error>
    where
        Self: Sized,
        // TODO: we should rely on `into` rather than `from`
        anyhow::Error: From<Self::Error>, {
        self.map_err(anyhow::Error::from)
    }
}

impl<T: ?Sized> TryStreamExt for T where T: TryStream {}
