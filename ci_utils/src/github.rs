use crate::prelude::*;

use octocrab::models::repos::Release;

const MAX_PER_PAGE: u8 = 100;

pub mod model;
pub mod release;

/// Entity that uniquely identifies a GitHub-hosted repository.
#[async_trait]
pub trait RepoPointer {
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
        client.all_pages(page).await
    }

    async fn latest_release(
        &self,
        client: &Octocrab,
    ) -> octocrab::Result<octocrab::models::repos::Release> {
        self.repos(client).releases().get_latest().await
    }

    async fn find_release_by_text(
        &self,
        client: &Octocrab,
        text: &str,
    ) -> anyhow::Result<octocrab::models::repos::Release> {
        let mut page = self.repos(client).releases().list().per_page(MAX_PER_PAGE).send().await?;
        // FIXME: iterate pages
        if let Some(total_count) = page.total_count {
            assert!(total_count < MAX_PER_PAGE.into());
        }

        page.take_items()
            .into_iter()
            .find(|release| release.tag_name.contains(text))
            .ok_or_else(|| anyhow!("No release with tag matching {}", text))
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
        .ok_or_else(|| anyhow!("Cannot find release asset by string {}", text))?;
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

    let arch_name = match platforms::TARGET_ARCH {
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
