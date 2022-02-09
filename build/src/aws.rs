use crate::paths::Paths;
use crate::prelude::*;
use ide_ci::models::config::RepoContext;

use aws_sdk_s3::model::ObjectCannedAcl;
use aws_sdk_s3::output::PutObjectOutput;
use aws_sdk_s3::ByteStream;
use aws_sdk_s3::Client;
use bytes::Buf;
use serde::de::DeserializeOwned;

/// The upper limit on number of nightly editions that are stored in the bucket.
pub const NIGHTLY_EDITIONS_LIMIT: usize = 20;

pub const EDITIONS_BUCKET_NAME: &str = "editions.release.enso.org";

pub const MANIFEST_FILENAME: &str = "manifest.yaml";



#[derive(Clone, Debug, Display, Serialize, Deserialize, Shrinkwrap)]
pub struct Edition(pub String);

impl AsRef<str> for Edition {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl<T: Into<String>> From<T> for Edition {
    fn from(value: T) -> Self {
        Edition(value.into())
    }
}

impl Edition {
    pub fn is_nightly(&self) -> bool {
        ide_ci::program::version::find_in_text(self)
            .as_ref()
            .map_or(false, crate::version::is_nightly)
    }
}



#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// Sequence of edition names.
    pub editions: Vec<Edition>,
}

impl Manifest {
    pub fn with_new_nightly(
        &self,
        new_nightly: Edition,
        nightlies_count_limit: usize,
    ) -> (Manifest, Vec<&Edition>) {
        let (nightlies, non_nightlies) =
            self.editions.iter().partition::<Vec<_>, _>(|e| e.is_nightly());
        let nightlies_count_to_remove = 1 + nightlies.len().saturating_sub(nightlies_count_limit);
        let (nightlies_to_remove, nightlies_to_keep) =
            nightlies.split_at(nightlies_count_to_remove);

        let mut new_editions = non_nightlies;
        new_editions.extend(nightlies_to_keep);
        new_editions.push(&new_nightly);

        let new_manifest = Manifest { editions: new_editions.into_iter().cloned().collect() };
        (new_manifest, nightlies_to_remove.to_vec())
    }
}


pub struct BucketContext {
    pub client:     aws_sdk_s3::Client,
    pub bucket:     String,
    pub upload_acl: ObjectCannedAcl,
    pub key_prefix: String,
}

impl BucketContext {
    pub async fn get(&self, path: &str) -> Result<ByteStream> {
        Ok(self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(format!("{}/{}", self.key_prefix, path))
            .send()
            .await?
            .body)
    }

    pub async fn put(&self, path: &str, data: ByteStream) -> Result<PutObjectOutput> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .acl(self.upload_acl.clone())
            .key(format!("{}/{}", self.key_prefix, path))
            .body(data)
            .send()
            .await
            .anyhow_err()
    }

    pub async fn get_yaml<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let text = self.get(path).await?.collect().await?;
        serde_yaml::from_reader(text.reader()).anyhow_err()
    }

    pub async fn put_yaml(&self, path: &str, data: &impl Serialize) -> Result<PutObjectOutput> {
        let buf = serde_yaml::to_vec(data)?;
        self.put(path, ByteStream::from(buf)).await
    }
}

pub async fn update_manifest(repo_context: &RepoContext, paths: &Paths) -> Result {
    let bucket_context = BucketContext {
        client:     Client::new(&aws_config::load_from_env().await),
        bucket:     EDITIONS_BUCKET_NAME.to_string(),
        upload_acl: ObjectCannedAcl::PublicRead,
        key_prefix: repo_context.name.clone(),
    };

    let new_edition_name = Edition(paths.edition_name());
    let new_edition_path = paths.edition_file();
    ensure!(
        new_edition_path.exists(),
        "The edition file {} does not exist.",
        new_edition_path.display()
    );

    let manifest = bucket_context.get_yaml::<Manifest>(MANIFEST_FILENAME).await?;


    let (new_manifest, nightlies_to_remove) =
        manifest.with_new_nightly(new_edition_name, NIGHTLY_EDITIONS_LIMIT);
    for nightly_to_remove in nightlies_to_remove {
        println!("Should remove {}", nightly_to_remove);
    }

    let new_edition_filename = new_edition_path.file_name().unwrap().to_str().unwrap();

    bucket_context
        .put(new_edition_filename, ByteStream::from_path(&new_edition_path).await?)
        .await?;

    bucket_context.put_yaml("manifest.yaml", &new_manifest).await?;
    Ok(())
}
