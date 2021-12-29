use crate::prelude::*;

#[derive(Copy, Clone, Debug)]
pub enum AsyncPolicy {
    Sequential,
    FutureParallelism,
    TaskParallelism,
}

pub async fn try_join_all<I, F: Future<Output = std::result::Result<T, E>>, T, E>(
    futures: I,
    parallel: AsyncPolicy,
) -> Result<Vec<T>>
where
    I: IntoIterator<Item = F>,
    F: Send + 'static,
    T: Send + 'static,
    E: Into<anyhow::Error> + Send + 'static,
{
    match parallel {
        AsyncPolicy::Sequential => {
            let mut ret = Vec::new();
            for future in futures {
                ret.push(future.await.anyhow_err()?);
            }
            Ok(ret)
        }
        AsyncPolicy::FutureParallelism => futures::future::try_join_all(futures).await.anyhow_err(),
        AsyncPolicy::TaskParallelism => {
            let tasks = futures
                .into_iter()
                .map(|future| async move { tokio::task::spawn(future).await?.anyhow_err() });
            futures::future::try_join_all(tasks).await
        }
    }
}
