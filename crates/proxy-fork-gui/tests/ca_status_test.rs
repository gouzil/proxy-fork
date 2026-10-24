use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{Action, App},
    platform::inspect_configured_ca,
    preferences::Preferences,
};

#[test]
fn only_a_missing_default_ca_is_not_configured() {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("ca.pem");
    assert_eq!(inspect_configured_ca(&cert, true).unwrap(), None);
    assert!(!cert.exists(), "inspection must not generate a CA");
    assert!(
        inspect_configured_ca(&cert, false).is_err(),
        "a missing custom path is an error"
    );
    let missing_parent = dir.path().join("missing").join("nested").join("ca.pem");
    assert_eq!(inspect_configured_ca(&missing_parent, true).unwrap(), None);
    assert!(
        !missing_parent.parent().unwrap().exists(),
        "inspection must not create directories"
    );
    fs_err::write(&cert, "not a certificate").unwrap();
    assert!(inspect_configured_ca(&cert, true).is_err());
    assert!(inspect_configured_ca(&cert, false).is_err());
    for invalid_parent in [cert.join("ca.pem"), cert.join("missing").join("ca.pem")] {
        assert!(
            inspect_configured_ca(&invalid_parent, true).is_err(),
            "non-directory path components must not be hidden"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = dir.path().join("denied");
        fs_err::create_dir(&parent).unwrap();
        let blocked = parent.join("ca.pem");
        fs_err::write(&blocked, "not a certificate").unwrap();
        fs_err::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        let denied = inspect_configured_ca(&blocked, true);
        fs_err::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            denied
                .unwrap_err()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::PermissionDenied,
            "permission errors must not be hidden"
        );
    }
}

#[test]
fn missing_ca_clears_stale_trust_but_keeps_recorded_ownership() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.ca_fingerprint = Some("old certificate".into());
    app.ca_keychain = Some("/test/login.keychain-db".into());
    app.ca_trusted = Some(true);
    app.ca_confirm = true;
    app.ca_overwrite = true;
    app.begin(Action::InspectCa).unwrap();
    app.ca_not_configured(Some("owned certificate".into()));
    app.finish(Ok("ca.not_configured".into()));
    assert!(app.ca_missing);
    assert!(app.error.is_none());
    assert_eq!(app.ca_fingerprint, None);
    assert_eq!(app.ca_keychain, None);
    assert_eq!(app.ca_trusted, None);
    assert!(!app.ca_confirm && !app.ca_overwrite && !app.can_trust_ca());
    assert_eq!(
        app.preferences.trusted_fingerprint.as_deref(),
        Some("owned certificate")
    );
}
