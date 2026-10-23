use anyhow::Result;
use std::path::Path;
pub fn autostart_status() -> Result<bool> {
    Err(super::Unsupported.into())
}
pub fn set_autostart(_: bool) -> Result<bool> {
    Err(super::Unsupported.into())
}
pub fn inspect_ca(cert: &Path) -> Result<(String, bool)> {
    Ok((proxy_fork_core::certificate_fingerprint(cert)?, false))
}
pub fn keychain_path() -> Result<String> {
    Err(super::Unsupported.into())
}
pub fn trust_ca(_: &Path, _: &str, _: &str) -> Result<(String, bool)> {
    Err(super::Unsupported.into())
}
pub fn remove_ca(_: &Path, _: &str) -> Result<()> {
    Err(super::Unsupported.into())
}
