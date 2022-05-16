use crate::prelude::*;
use anyhow::Context;
use headers::HeaderMap;
use headers::HeaderValue;
use octocrab::models::ArtifactId;
use octocrab::models::AssetId;
use octocrab::models::ReleaseId;
use octocrab::models::RunId;
use std::io::Cursor;

// use crate::global::new_spinner;
use crate::cache::download::DownloadFile;
use octocrab::models::repos::Asset;
use octocrab::models::repos::Release;
use octocrab::models::workflows::WorkflowListArtifact;
use octocrab::params::actions::ArchiveFormat;
use reqwest::Response;

const MAX_PER_PAGE: u8 = 100;

pub mod model;
pub mod release;

/// Entity that uniquely identifies a GitHub-hosted repository.
#[async_trait]
pub trait RepoPointer: Display {
    fn owner(&self) -> &str;
    fn name(&self) -> &str;

    /// Generate a token that can be used to register a new runner for this repository.
    async fn generate_runner_registration_token(
        &self,
        octocrab: &Octocrab,
    ) -> anyhow::Result<model::RegistrationToken> {
        let path =
            iformat!("/repos/{self.owner()}/{self.name()}/actions/runners/registration-token");
        let url = octocrab.absolute_url(path)?;
        octocrab.post(url, EMPTY_REQUEST_BODY).await.map_err(Into::into)
    }

    /// The repository's URL.
    fn url(&self) -> anyhow::Result<Url> {
        let url_text = iformat!("https://github.com/{self.owner()}/{self.name()}");
        Url::parse(&url_text).map_err(Into::into)
    }

    fn repos<'a>(&'a self, client: &'a Octocrab) -> octocrab::repos::RepoHandler<'a> {
        client.repos(self.owner(), self.name())
    }

    async fn all_releases(
        &self,
        client: &Octocrab,
    ) -> octocrab::Result<Vec<octocrab::models::repos::Release>> {
        let repo = self.repos(client);
        let page = repo.releases().list().per_page(MAX_PER_PAGE).send().await?;
        // TODO: rate limit?
        //       it should be possible to have stream of releases that fetches pages as needed
        client.all_pages(page).await
    }

    async fn latest_release(
        &self,
        client: &Octocrab,
    ) -> octocrab::Result<octocrab::models::repos::Release> {
        self.repos(client).releases().get_latest().await
    }

    async fn find_release_by_id(
        &self,
        client: &Octocrab,
        release_id: ReleaseId,
    ) -> Result<octocrab::models::repos::Release> {
        let repo_handler = self.repos(client);
        let releases_handler = repo_handler.releases();
        releases_handler
            .get_by_id(release_id)
            .await
            .context(format!("Failed to find release by id `{release_id}` in `{self}`."))
    }

    async fn find_release_by_text(
        &self,
        client: &Octocrab,
        text: &str,
    ) -> anyhow::Result<octocrab::models::repos::Release> {
        self.all_releases(client)
            .await?
            .into_iter()
            .find(|release| release.tag_name.contains(text))
            .ok_or_else(|| anyhow!("No release with tag matching {}", text))
    }

    async fn find_artifact_by_name(
        &self,
        client: &Octocrab,
        run_id: RunId,
        name: &str,
    ) -> Result<WorkflowListArtifact> {
        let artifacts = client
            .actions()
            .list_workflow_run_artifacts(self.owner(), self.name(), run_id)
            .per_page(100)
            .send()
            .await
            .context(format!("Failed to list artifacts of run {run_id} in {self}."))?
            .value
            .context(format!("Failed to find any artifacts."))?;

        artifacts
            .into_iter()
            .find(|artifact| artifact.name == name)
            .context(format!("Failed to find artifact by name '{name}'."))
    }

    async fn download_artifact(&self, client: &Octocrab, artifact_id: ArtifactId) -> Result<Bytes> {
        // let _bar = new_spinner(format!("Downloading artifact id={}", artifact_id));
        client
            .actions()
            .download_artifact(self.owner(), self.name(), artifact_id, ArchiveFormat::Zip)
            .await
            .anyhow_err()
    }

    async fn download_and_unpack_artifact(
        &self,
        client: &Octocrab,
        artifact_id: ArtifactId,
        output_dir: &Path,
    ) -> Result {
        let bytes = self.download_artifact(client, artifact_id).await?;
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        archive.extract(output_dir)?;
        Ok(())
    }

    #[tracing::instrument(name="Get the asset information.", skip(client), fields(self=%self), err)]
    async fn asset(&self, client: &Octocrab, asset_id: AssetId) -> Result<Asset> {
        self.repos(client).releases().get_asset(asset_id).await.anyhow_err()
    }

    fn download_asset_job(&self, octocrab: &Octocrab, asset_id: AssetId) -> DownloadFile {
        let path = iformat!("/repos/{self.owner()}/{self.name()}/releases/assets/{asset_id}");
        // Unwrap will work, because we are appending relative URL constant.
        let url = octocrab.absolute_url(path).unwrap();
        crate::cache::download::DownloadFile {
            client: octocrab.client.clone(),
            key:    crate::cache::download::Key {
                url,
                additional_headers: HeaderMap::from_iter([(
                    reqwest::header::ACCEPT,
                    HeaderValue::from_static(mime::APPLICATION_OCTET_STREAM.as_ref()),
                )]),
            },
        }
    }

    #[tracing::instrument(name="Download the asset.", skip(client), fields(self=%self), err)]
    async fn download_asset(&self, client: &Octocrab, asset_id: AssetId) -> Result<Response> {
        self.download_asset_job(client, asset_id).send_request().await
    }

    #[tracing::instrument(name="Download the asset to a file.", skip(client,output_path), fields(self=%self, dest=%output_path.as_ref().display()), err)]
    async fn download_asset_as(
        &self,
        client: &Octocrab,
        asset_id: AssetId,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> Result {
        let response = self.download_asset(client, asset_id).await?;
        crate::io::web::stream_response_to_file(response, &output_path).await
    }
}

#[async_trait]
pub trait OrganizationPointer {
    /// Organization name.
    fn name(&self) -> &str;

    /// Generate a token that can be used to register a new runner for this repository.
    async fn generate_runner_registration_token(
        &self,
        octocrab: &Octocrab,
    ) -> anyhow::Result<model::RegistrationToken> {
        let path = iformat!("/orgs/{self.name()}/actions/runners/registration-token");
        let url = octocrab.absolute_url(path)?;
        octocrab.post(url, EMPTY_REQUEST_BODY).await.map_err(Into::into)
    }

    /// The organization's URL.
    fn url(&self) -> Result<Url> {
        let url_text = iformat!("https://github.com/{self.name()}");
        Url::parse(&url_text).map_err(Into::into)
    }
}

/// Get the biggest asset containing given text.
pub fn find_asset_url_by_text<'a>(release: &'a Release, text: &str) -> anyhow::Result<&'a Url> {
    let matching_asset = release
        .assets
        .iter()
        .filter(|asset| asset.name.contains(&text))
        .max_by_key(|asset| asset.size)
        .ok_or_else(|| {
            anyhow!("Cannot find release asset by string {} in the release {}.", text, release.url)
        })?;
    Ok(&matching_asset.browser_download_url)
}

/// Obtain URL to an archive with the latest runner package for a given system.
///
/// Octocrab client does not need to bo authorized with a PAT for this. However, being authorized
/// will help with GitHub API query rate limits.
pub async fn latest_runner_url(octocrab: &Octocrab, os: OS) -> anyhow::Result<Url> {
    let latest_release = octocrab.repos("actions", "runner").releases().get_latest().await?;

    let os_name = match os {
        OS::Linux => "linux",
        OS::Windows => "win",
        OS::MacOS => "osx",
        other_os => unimplemented!("System `{}` is not yet supported!", other_os),
    };

    let arch_name = match TARGET_ARCH {
        Arch::X86_64 => "x64",
        Arch::Arm => "arm",
        Arch::AArch64 if os == OS::MacOS => "x64", /* M1 native runners are not yet supported, see: https://github.com/actions/runner/issues/805 */
        Arch::AArch64 => "arm64",
        other_arch => unimplemented!("Architecture `{}` is not yet supported!", other_arch),
    };

    let platform_name = format!("{}-{}", os_name, arch_name);
    find_asset_url_by_text(&latest_release, &platform_name).cloned()
}

pub async fn fetch_runner(octocrab: &Octocrab, os: OS, output_dir: impl AsRef<Path>) -> Result {
    let url = latest_runner_url(octocrab, os).await?;
    crate::io::download_and_extract(url, output_dir).await
}

/// Sometimes octocrab is just not enough.
///
/// Client has set the authorization header.
pub fn create_client(pat: impl AsRef<str>) -> Result<reqwest::Client> {
    let mut header_map = reqwest::header::HeaderMap::new();
    header_map.append(reqwest::header::AUTHORIZATION, format!("Bearer {}", pat.as_ref()).parse()?);
    reqwest::Client::builder()
        .user_agent("enso-build")
        .default_headers(header_map)
        .build()
        .anyhow_err()
}
