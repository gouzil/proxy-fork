use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
pub const BUNDLE_ID: &str = "io.github.gouzil.proxy-fork";
#[derive(Debug)]
pub struct Unsupported;
impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("platform service is unsupported")
    }
}
impl std::error::Error for Unsupported {}
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub use unsupported::*;

pub fn inspect_configured_ca(cert: &Path, is_default: bool) -> Result<Option<(String, bool)>> {
    match fs_err::metadata(cert) {
        Err(error) if is_default && error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(_) => inspect_ca(cert).map(Some),
    }
}

pub fn bundle_path() -> Result<PathBuf> {
    std::env::current_exe()?
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .context("operation requires an installed .app bundle")
}

pub fn command_output(program: &str, args: &[&std::ffi::OsStr]) -> Result<String> {
    let output = std::process::Command::new(program).args(args).output()?;
    if !output.status.success() {
        let stderr: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(2000)
            .collect();
        anyhow::bail!("{program} failed ({}): {stderr}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_matches('"')
        .to_string())
}
