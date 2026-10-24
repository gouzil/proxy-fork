use anyhow::{Result, ensure};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum KeychainError {
    PermissionDenied,
    NotFound,
    InvalidOutput,
    CommandFailed(String),
}
impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermissionDenied => f.write_str("Keychain authorization was denied or cancelled"),
            Self::NotFound => f.write_str("certificate or Keychain was not found"),
            Self::InvalidOutput => f.write_str("invalid Keychain command output"),
            Self::CommandFailed(message) => write!(f, "Keychain command failed: {message}"),
        }
    }
}
impl std::error::Error for KeychainError {}

fn security_error(code: Option<i32>, stderr: &str) -> KeychainError {
    match code {
        Some(128 | 51 | 36) => KeychainError::PermissionDenied,
        Some(44) => KeychainError::NotFound,
        _ => KeychainError::CommandFailed(stderr.chars().take(2000).collect()),
    }
}
fn security_output(args: &[&std::ffi::OsStr]) -> Result<String> {
    let output = std::process::Command::new("/usr/bin/security")
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(security_error(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)
        .map_err(|_| KeychainError::InvalidOutput)?
        .trim()
        .trim_matches('"')
        .into())
}
pub fn keychain_path() -> Result<String> {
    let path = security_output(&["default-keychain".as_ref(), "-d".as_ref(), "user".as_ref()])?;
    ensure!(path.starts_with('/'), KeychainError::InvalidOutput);
    Ok(path)
}
pub fn inspect_ca(cert: &Path) -> Result<(String, bool)> {
    let fingerprint = proxy_fork_core::certificate_fingerprint(cert)?;
    let keychain = keychain_path()?;
    let certificates = security_output(&[
        "find-certificate".as_ref(),
        "-a".as_ref(),
        "-Z".as_ref(),
        keychain.as_ref(),
    ])
    .or_else(|error| {
        if error.downcast_ref::<KeychainError>() == Some(&KeychainError::NotFound) {
            Ok(String::new())
        } else {
            Err(error)
        }
    })?;
    let present = certificates.lines().any(|line| {
        line.trim()
            .strip_prefix("SHA-256 hash:")
            .is_some_and(|hash| hash.trim().eq_ignore_ascii_case(&fingerprint))
    });
    let trusted = present
        && std::process::Command::new("/usr/bin/security")
            .arg("verify-cert")
            .arg("-c")
            .arg(cert)
            .args(["-p", "ssl"])
            .output()?
            .status
            .success();
    Ok((fingerprint, trusted))
}
pub fn trust_ca(cert: &Path, expected: &str, target_keychain: &str) -> Result<(String, bool)> {
    ensure!(
        proxy_fork_core::certificate_fingerprint(cert)? == expected,
        "certificate changed since confirmation"
    );
    let keychain = keychain_path()?;
    ensure!(
        keychain == target_keychain,
        "target Keychain changed since confirmation; inspect it again"
    );
    security_output(&[
        "add-trusted-cert".as_ref(),
        "-r".as_ref(),
        "trustRoot".as_ref(),
        "-k".as_ref(),
        keychain.as_ref(),
        cert.as_os_str(),
    ])?;
    let result = inspect_ca(cert)?;
    ensure!(
        result.1,
        "certificate installed but trust verification failed"
    );
    Ok(result)
}
pub fn remove_ca(cert: &Path, recorded: &str) -> Result<()> {
    ensure!(
        proxy_fork_core::certificate_fingerprint(cert)? == recorded,
        "only the recorded application certificate can be removed"
    );
    let keychain = keychain_path()?;
    let certificates = security_output(&[
        "find-certificate".as_ref(),
        "-a".as_ref(),
        "-Z".as_ref(),
        keychain.as_ref(),
    ])
    .or_else(|error| {
        if error.downcast_ref::<KeychainError>() == Some(&KeychainError::NotFound) {
            Ok(String::new())
        } else {
            Err(error)
        }
    })?;
    let Some(sha1) = matching_sha1(&certificates, recorded) else {
        return Ok(());
    };
    security_output(&[
        "delete-certificate".as_ref(),
        "-Z".as_ref(),
        sha1.as_ref(),
        keychain.as_ref(),
    ])?;
    Ok(())
}
fn matching_sha1(output: &str, sha256: &str) -> Option<String> {
    let mut matches = false;
    for line in output.lines() {
        if let Some(hash) = line.trim().strip_prefix("SHA-256 hash:") {
            matches = hash.trim().eq_ignore_ascii_case(sha256);
        } else if matches && let Some(hash) = line.trim().strip_prefix("SHA-1 hash:") {
            let hash = hash.trim();
            if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(hash.into());
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deletion_only_selects_the_matching_certificate() {
        let output = "SHA-256 hash: OTHER\nSHA-1 hash: 1111111111111111111111111111111111111111\nSHA-256 hash: OURS\nSHA-1 hash: 2222222222222222222222222222222222222222\n";
        assert_eq!(
            matching_sha1(output, "OURS").unwrap(),
            "2222222222222222222222222222222222222222"
        );
        assert!(matching_sha1(output, "missing").is_none());
    }
    #[test]
    fn keychain_errors_distinguish_cancellation_absence_and_other_failures() {
        assert_eq!(
            security_error(Some(128), "cancelled"),
            KeychainError::PermissionDenied
        );
        assert_eq!(security_error(Some(44), "missing"), KeychainError::NotFound);
        assert!(matches!(
            security_error(Some(1), "bad command"),
            KeychainError::CommandFailed(_)
        ));
    }
}
