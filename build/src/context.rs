use crate::prelude::*;

use crate::paths::generated::RepoRoot;
use crate::paths::TargetTriple;
use derivative::Derivative;
use ide_ci::cache::Cache;
use ide_ci::models::config::RepoContext;
use ide_ci::programs::Git;


/// The basic, common information available in this application.
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct BuildContext {
    /// GitHub API client.
    ///
    /// If authorized, it will count API rate limits against our identity and allow operations like
    /// managing releases or downloading CI run artifacts.
    #[derivative(Debug = "ignore")]
    pub octocrab: Octocrab,

    /// Version to be built.
    ///
    /// Note that this affects only targets that are being built. If project parts are provided by
    /// other means, their version might be different.
    pub triple: TargetTriple,

    /// Directory being an `enso` repository's working copy.
    ///
    /// The directory is not required to be a git repository. It is allowed to use source tarballs
    /// as well.
    pub source_root: PathBuf,

    /// Remote repository is used for release-related operations. This also includes deducing a new
    /// version number.
    pub remote_repo: RepoContext,

    /// Stores things like downloaded release assets to save time.
    pub cache: Cache,
}

impl BuildContext {
    pub fn repo_root(&self) -> RepoRoot {
        RepoRoot::new(
            &self.source_root,
            &self.triple.to_string(),
            &self.triple.versions.edition_name(),
        )
    }

    pub fn commit(&self) -> BoxFuture<'static, Result<String>> {
        let root = self.source_root.clone();
        async move {
            match ide_ci::actions::env::GITHUB_SHA.get() {
                Ok(commit) => Ok(commit),
                Err(_e) => Git::new(root).head_hash().await,
            }
        }
        .boxed()
    }
}
