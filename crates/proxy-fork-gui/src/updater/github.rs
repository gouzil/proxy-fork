use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    assets: Vec<Asset>,
}
#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub asset_name: String,
    pub url: String,
    pub checksum_url: String,
    pub size: u64,
}

pub fn parse_release(json: &str, current: &str, target: &str) -> Result<Option<Release>> {
    let release: GithubRelease = serde_json_parse(json)?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = release
        .tag_name
        .strip_prefix('v')
        .context("release tag must start with v")?;
    let parsed = Version::parse(version)?;
    if !parsed.pre.is_empty() || parsed <= Version::parse(current)? {
        return Ok(None);
    }
    let asset_name = format!("proxy-fork-gui-{version}-{target}.app.tar.gz");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .context("release has no compatible application asset")?;
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == format!("{asset_name}.sha256"))
        .context("release has no checksum asset")?;
    let prefix = format!(
        "https://github.com/gouzil/proxy-fork/releases/download/{}/",
        release.tag_name
    );
    ensure!(
        asset.browser_download_url == format!("{prefix}{asset_name}")
            && checksum.browser_download_url == format!("{prefix}{asset_name}.sha256"),
        "unexpected update download origin"
    );
    ensure!(
        asset.size > 0 && asset.size <= 512 * 1024 * 1024,
        "invalid application asset size"
    );
    Ok(Some(Release {
        version: version.into(),
        notes: release.body.unwrap_or_default(),
        asset_name,
        url: asset.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        size: asset.size,
    }))
}
fn serde_json_parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T> {
    Ok(serde_json::from_str(json)?)
}

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .user_agent(concat!("proxy-fork-gui/", env!("CARGO_PKG_VERSION")))
        .build()?)
}
pub async fn check() -> Result<Option<Release>> {
    anyhow::ensure!(
        cfg!(target_os = "macos"),
        "updates are not implemented for this platform"
    );
    fetch_latest(
        &client()?,
        "https://api.github.com/repos/gouzil/proxy-fork/releases/latest",
    )
    .await
}
async fn bounded_text(mut response: reqwest::Response, limit: usize) -> Result<String> {
    ensure!(
        response.content_length().unwrap_or(0) <= limit as u64,
        "oversized update metadata"
    );
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            chunk.len() <= limit.saturating_sub(body.len()),
            "oversized update metadata"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8(body)?)
}
async fn fetch_latest(client: &reqwest::Client, url: &str) -> Result<Option<Release>> {
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    parse_release(
        &bounded_text(response, 2 * 1024 * 1024).await?,
        env!("CARGO_PKG_VERSION"),
        super::target(),
    )
}

pub async fn download(
    release: Release,
    state_dir: std::path::PathBuf,
    progress: impl Fn(u64, Option<u64>),
) -> Result<super::VerifiedUpdate> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;
    let client = client()?;
    let response = client
        .get(&release.checksum_url)
        .send()
        .await?
        .error_for_status()?;
    let checksum_text = bounded_text(response, 8192).await?;
    let expected = super::expected_checksum(&checksum_text, &release.asset_name)?;
    let updates = state_dir.join("updates");
    tokio::fs::create_dir_all(&updates).await?;
    let directory = tempfile::Builder::new()
        .prefix("download-")
        .tempdir_in(updates)?;
    let archive = directory.path().join("update.tar.gz");
    let mut file = tokio::fs::File::create(&archive).await?;
    let mut response = client.get(&release.url).send().await?.error_for_status()?;
    let total = response.content_length();
    let mut received = 0u64;
    let mut digest = Sha256::new();
    let mut last_progress = std::time::Instant::now();
    while let Some(bytes) = response.chunk().await? {
        received += bytes.len() as u64;
        ensure!(
            received <= release.size && received <= 512 * 1024 * 1024,
            "update exceeds declared size"
        );
        digest.update(&bytes);
        file.write_all(&bytes).await?;
        if last_progress.elapsed() >= std::time::Duration::from_millis(100) {
            progress(received, total);
            last_progress = std::time::Instant::now();
        }
    }
    ensure!(received == release.size, "incomplete update download");
    file.sync_all().await?;
    drop(file);
    let actual: String = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    ensure!(actual == expected, "update checksum mismatch");
    progress(received, total);
    tokio::task::spawn_blocking(move || {
        let bundle = super::unpack_bundle(&archive, directory.path())?;
        super::verify_bundle(&bundle, &release.version, &crate::platform::bundle_path()?)?;
        Ok(super::VerifiedUpdate {
            directory,
            bundle,
            version: release.version,
        })
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve(status: &str, body: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!("HTTP/1.1 {status}\r\nConnection: close\r\n\r\n{body}");
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 2048];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            let _ = stream.write_all(response.as_bytes()).await;
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn release_http_errors_and_unbounded_metadata_are_rejected() {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        assert!(
            fetch_latest(&client, &serve("404 Not Found", "").await)
                .await
                .unwrap()
                .is_none()
        );
        for status in ["429 Too Many Requests", "500 Server Error"] {
            assert!(
                fetch_latest(&client, &serve(status, "error").await)
                    .await
                    .is_err()
            );
        }
        assert!(
            fetch_latest(
                &client,
                &serve("200 OK", &"x".repeat(2 * 1024 * 1024 + 1)).await
            )
            .await
            .is_err()
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let offline = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert!(fetch_latest(&client, &offline).await.is_err());
    }

    #[tokio::test]
    async fn release_http_selects_only_new_stable_architecture_assets() {
        let client = reqwest::Client::new();
        let asset = format!("proxy-fork-gui-9.0.0-{}.app.tar.gz", super::super::target());
        let origin = "https://github.com/gouzil/proxy-fork/releases/download/v9.0.0";
        let mut json = serde_json::json!({"tag_name":"v9.0.0", "assets":[{"name": asset, "browser_download_url":format!("{origin}/{asset}"),"size":100},{"name":format!("{asset}.sha256"),"browser_download_url":format!("{origin}/{asset}.sha256"),"size":100}]});
        let release = fetch_latest(&client, &serve("200 OK", &json.to_string()).await)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(release.version, "9.0.0");
        json["prerelease"] = true.into();
        assert!(
            fetch_latest(&client, &serve("200 OK", &json.to_string()).await)
                .await
                .unwrap()
                .is_none()
        );
        json["prerelease"] = false.into();
        json["tag_name"] = format!("v{}", env!("CARGO_PKG_VERSION")).into();
        assert!(
            fetch_latest(&client, &serve("200 OK", &json.to_string()).await)
                .await
                .unwrap()
                .is_none()
        );
    }
}
