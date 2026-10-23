use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub fn expected_checksum(text: &str, asset: &str) -> Result<String> {
    let mut fields = text.split_whitespace();
    let hash = fields.next().context("empty checksum")?;
    ensure!(
        hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
        "invalid SHA-256 checksum"
    );
    if let Some(name) = fields.next() {
        ensure!(
            name.trim_start_matches('*') == asset,
            "checksum names another asset"
        );
    }
    ensure!(fields.next().is_none(), "ambiguous checksum file");
    Ok(hash.to_ascii_lowercase())
}
pub fn verify_checksum(bytes: &[u8], text: &str, asset: &str) -> Result<()> {
    let actual: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    ensure!(
        actual == expected_checksum(text, asset)?,
        "update checksum mismatch"
    );
    Ok(())
}
pub fn unpack_bundle(archive: &Path, destination: &Path) -> Result<PathBuf> {
    let decoder = flate2::read::GzDecoder::new(fs_err::File::open(archive)?);
    let mut archive = tar::Archive::new(decoder);
    let mut expanded = 0u64;
    for (index, entry) in archive.entries()?.enumerate() {
        ensure!(index < 10000, "update contains too many files");
        let mut entry = entry?;
        let path = entry.path()?;
        let mut components = path.components();
        ensure!(
            components.next() == Some(std::path::Component::Normal("Proxy Fork.app".as_ref()))
                && components.all(|p| matches!(p, std::path::Component::Normal(_))),
            "update path is outside the expected application bundle: {}",
            path.display()
        );
        ensure!(
            entry.header().entry_type().is_file() || entry.header().entry_type().is_dir(),
            "update contains unsupported links or special files"
        );
        expanded = expanded
            .checked_add(entry.size())
            .context("archive size overflow")?;
        ensure!(
            expanded <= 1024 * 1024 * 1024,
            "expanded update is too large"
        );
        ensure!(
            entry.unpack_in(destination)?,
            "archive path escapes destination"
        );
    }
    let bundles: Vec<_> = fs_err::read_dir(destination)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .collect();
    ensure!(
        bundles.len() == 1,
        "update must contain exactly one application bundle"
    );
    Ok(bundles[0].clone())
}

#[cfg(target_os = "macos")]
pub fn signature_team(bundle: &Path) -> Result<String> {
    let result = std::process::Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(bundle)
        .output()?;
    ensure!(
        result.status.success(),
        "application signature metadata is unavailable"
    );
    String::from_utf8_lossy(&result.stderr)
        .lines()
        .find_map(|s| s.strip_prefix("TeamIdentifier="))
        .filter(|s| !s.is_empty() && *s != "not set")
        .map(str::to_owned)
        .context("a Developer ID signed application is required for automatic updates")
}
#[cfg(target_os = "macos")]
pub fn verify_bundle(bundle: &Path, version: &str, installed: &Path) -> Result<()> {
    use crate::platform::{BUNDLE_ID, command_output};
    command_output(
        "/usr/bin/codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            bundle.as_os_str(),
        ],
    )?;
    command_output(
        "/usr/sbin/spctl",
        &[
            "--assess".as_ref(),
            "--type".as_ref(),
            "execute".as_ref(),
            bundle.as_os_str(),
        ],
    )?;
    ensure!(
        signature_team(bundle)? == signature_team(installed)?,
        "update signing team differs from installed application"
    );
    let value = plist::Value::from_file(bundle.join("Contents/Info.plist"))?;
    let info = value.as_dictionary().context("invalid bundle metadata")?;
    ensure!(
        info.get("CFBundleIdentifier")
            .and_then(plist::Value::as_string)
            == Some(BUNDLE_ID),
        "update bundle identifier mismatch"
    );
    ensure!(
        info.get("CFBundleShortVersionString")
            .and_then(plist::Value::as_string)
            == Some(version),
        "update version mismatch"
    );
    Ok(())
}
#[cfg(not(target_os = "macos"))]
pub fn verify_bundle(_: &Path, _: &str, _: &Path) -> Result<()> {
    anyhow::bail!("update installation is unsupported on this platform")
}
