use crate::prelude::*;

use crate::cache::Cache;
use crate::cache::Storable;


#[derive(Clone, Debug)]
pub struct ExtractedArchive<S> {
    pub archive_source: S,
}

// Unfortunately this impl cannot be simplified to just:
// impl<S: Storable> Borrow<S::Key> for ExtractedArchive<S>
// See: https://stackoverflow.com/questions/53021939/how-can-i-implement-borrow-for-a-generic-container-in-the-case-of-the-use-of-ass
// See: https://github.com/rust-lang/rust/issues/50237
// Because of that we must introduce spurious `K` type parameter and repeat Key's constraint.
impl<S, K> Borrow<K> for ExtractedArchive<S>
where
    S: Storable<Key = K>,
    K: Clone + Debug + Serialize + DeserializeOwned + 'static,
{
    fn borrow(&self) -> &K {
        self.archive_source.borrow()
    }
}

impl<S: Storable<Output = PathBuf> + Clone> Storable for ExtractedArchive<S> {
    type Metadata = ();
    type Output = PathBuf;
    type Key = S::Key;

    fn generate(
        &self,
        cache: Cache,
        store: PathBuf,
    ) -> BoxFuture<'static, crate::Result<Self::Metadata>> {
        let get_archive_job = cache.get(self.archive_source.clone());
        async move {
            let archive_path = get_archive_job.await?;
            // // FIXME: hardcoded bundle directory name
            crate::archive::extract_item(&archive_path, "enso", &store).await
        }
        .boxed()
    }

    fn adapt(
        &self,
        cache: PathBuf,
        _: Self::Metadata,
    ) -> BoxFuture<'static, crate::Result<Self::Output>> {
        async move { Ok(cache) }.boxed()
    }
}
