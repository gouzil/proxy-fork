use anyhow::{Result, ensure};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum KeychainError {
    Expired,
    PermissionDenied,
    NotFound,
    InvalidOutput,
    CommandFailed(String),
}
impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Expired => f.write_str(&rust_i18n::t!("ca.expired_error")),
            Self::PermissionDenied => f.write_str(&rust_i18n::t!("ca.authorization_error")),
            Self::NotFound => f.write_str(&rust_i18n::t!("ca.not_found_error")),
            Self::InvalidOutput => f.write_str(&rust_i18n::t!("ca.invalid_output_error")),
            Self::CommandFailed(message) => {
                write!(f, "{}: {message}", rust_i18n::t!("ca.keychain_error"))
            }
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
    let bytes = fs_err::read(cert)?;
    let certificate =
        openssl::x509::X509::from_pem(&bytes).or_else(|_| openssl::x509::X509::from_der(&bytes))?;
    let now = openssl::asn1::Asn1Time::days_from_now(0)?;
    ensure!(
        certificate.not_after().compare(&now)?.is_gt(),
        KeychainError::Expired
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
    // User trust survives deleting the Keychain item, so revoke it even on retries.
    security_output(&["remove-trusted-cert".as_ref(), cert.as_os_str()]).or_else(|error| {
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
    fn expired_ca_is_rejected_before_keychain_changes() {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("expired.pem");
        let config = proxy_fork_core::SelfSignedCaBuilder::default()
            .validity_days(-1)
            .build()
            .unwrap();
        let ca = proxy_fork_core::SelfSignedCa::gen_signed_cert(&config).unwrap();
        fs_err::write(&cert, ca.certificate.pem()).unwrap();
        let fingerprint = proxy_fork_core::certificate_fingerprint(&cert).unwrap();
        let error = trust_ca(&cert, &fingerprint, "/unused-test-keychain").unwrap_err();
        assert_eq!(
            error.downcast_ref::<KeychainError>(),
            Some(&KeychainError::Expired)
        );
    }
    #[test]
    #[ignore = "requires an authorized CA fixture in PROXY_FORK_E2E_CA_CERT"]
    fn removal_revokes_user_trust_without_a_keychain_item() {
        let cert = std::path::PathBuf::from(
            std::env::var_os("PROXY_FORK_E2E_CA_CERT").expect("set the authorized CA fixture path"),
        );
        let fingerprint = proxy_fork_core::certificate_fingerprint(&cert).unwrap();
        remove_ca(&cert, &fingerprint).unwrap();
        let output = std::process::Command::new("/usr/bin/security")
            .args(["verify-cert", "-p", "ssl", "-c"])
            .arg(&cert)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "the removed CA must no longer be trusted"
        );
    }
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
