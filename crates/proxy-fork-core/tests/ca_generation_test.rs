use proxy_fork_core::certification::generate_ca;

#[test]
fn generation_does_not_overwrite_keys_without_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("ca.pem");
    let key = dir.path().join("key.pem");
    let generated = generate_ca(&cert, &key, false).unwrap();
    assert_eq!(generated.fingerprint.len(), 64);
    let before = fs_err::read(&key).unwrap();
    assert!(generate_ca(&cert, &key, false).is_err());
    assert_eq!(fs_err::read(&key).unwrap(), before);
    generate_ca(&cert, &key, true).unwrap();
    assert_ne!(fs_err::read(&key).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs_err::metadata(&key).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn failed_certificate_replacement_preserves_the_previous_private_key() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("certificate-directory");
    let key = dir.path().join("key.pem");
    fs_err::create_dir(&cert).unwrap();
    fs_err::write(&key, b"previous private key").unwrap();
    assert!(generate_ca(&cert, &key, true).is_err());
    assert_eq!(fs_err::read(&key).unwrap(), b"previous private key");
    assert!(cert.is_dir());
}
