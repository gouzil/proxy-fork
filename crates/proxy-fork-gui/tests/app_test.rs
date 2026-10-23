use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{Action, App},
    preferences::{Preferences, ThemePreference},
};

#[test]
fn duplicate_actions_are_rejected_and_language_preserves_rule_drafts() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "未保存.example".into();
    let id = app.rules[0].id;
    assert!(app.begin(Action::Start).is_some());
    assert!(app.begin(Action::Start).is_none());
    app.set_locale("zh-CN");
    assert_eq!(app.rules[0].rule.host, "未保存.example");
    assert_eq!(app.rules[0].id, id);
    app.finish(Ok("complete".into()));
    assert!(app.begin(Action::Stop).is_some());
}

#[test]
fn preferences_round_trip_and_corruption_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.toml");
    let prefs = Preferences {
        locale: "zh-CN".into(),
        ..Preferences::default()
    };
    prefs.save(&path).unwrap();
    assert_eq!(Preferences::load(&path).unwrap().locale, "zh-CN");
    fs_err::write(&path, "not valid toml").unwrap();
    assert!(Preferences::load(&path).is_err());
}

#[test]
fn legacy_preferences_follow_system_and_theme_choice_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.toml");
    fs_err::write(&path, "locale = 'zh-CN'\nwindow_size = [940, 668]\n").unwrap();
    let preferences = Preferences::load(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&preferences).unwrap()["theme"],
        "system"
    );
    for theme in ["light", "dark", "system"] {
        fs_err::write(&path, format!("locale = 'en'\ntheme = '{theme}'\n")).unwrap();
        Preferences::load(&path).unwrap().save(&path).unwrap();
        let restored = Preferences::load(&path).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap()["theme"], theme);
    }
    fs_err::write(&path, "theme = 'unsupported'").unwrap();
    assert!(Preferences::load(&path).is_err());
}

#[test]
fn failed_operation_still_saves_a_queued_theme_without_hiding_its_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.toml");
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.begin(Action::CheckUpdate).unwrap();
    app.preferences.theme = ThemePreference::Dark;
    app.preferences_dirty = true;
    assert_eq!(app.queued_action(), None);

    app.finish(Err("update server unavailable".into()));
    assert_eq!(app.queued_action(), Some(Action::SavePreferences));
    app.begin(app.queued_action().unwrap()).unwrap();
    assert_eq!(app.error.as_deref(), Some("update server unavailable"));
    app.preferences.save(&path).unwrap();
    app.finish(Ok("saved".into()));

    assert_eq!(
        Preferences::load(&path).unwrap().theme,
        ThemePreference::Dark
    );
    assert_eq!(app.error.as_deref(), Some("update server unavailable"));
    assert_eq!(app.queued_action(), None);
}

#[test]
fn failed_automatic_save_reports_both_errors_without_retrying_forever() {
    let dir = tempfile::tempdir().unwrap();
    let blocked_path = dir.path().join("gui.toml");
    fs_err::create_dir(&blocked_path).unwrap();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.begin(Action::CheckUpdate).unwrap();
    app.preferences.theme = ThemePreference::Dark;
    app.preferences_dirty = true;
    app.finish(Err("update server unavailable".into()));
    app.begin(app.queued_action().unwrap()).unwrap();
    let error = app.preferences.save(&blocked_path).unwrap_err().to_string();
    app.finish(Err(error.clone()));

    let reported = app.error.as_deref().unwrap();
    assert!(reported.contains("update server unavailable"));
    assert!(reported.contains(&error));
    assert_eq!(app.queued_action(), None);
    assert!(!app.busy);
}

#[test]
fn a_new_theme_during_a_failed_save_is_saved_once_afterwards() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.toml");
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.preferences.theme = ThemePreference::Light;
    app.preferences_dirty = true;
    app.begin(Action::SavePreferences).unwrap();
    app.preferences.theme = ThemePreference::Dark;
    app.preferences_dirty = true;
    assert_eq!(app.queued_action(), None);

    app.finish(Err("first save failed".into()));
    assert_eq!(app.queued_action(), Some(Action::SavePreferences));
    app.begin(app.queued_action().unwrap()).unwrap();
    app.preferences.save(&path).unwrap();
    app.finish(Ok("saved".into()));

    assert_eq!(
        Preferences::load(&path).unwrap().theme,
        ThemePreference::Dark
    );
    assert_eq!(app.error.as_deref(), Some("first save failed"));
    assert_eq!(app.queued_action(), None);
}

#[test]
fn a_manual_preferences_save_can_clear_an_old_error() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.begin(Action::CheckUpdate).unwrap();
    app.finish(Err("old operation failed".into()));
    app.begin(Action::SavePreferences).unwrap();
    assert_eq!(app.error, None);
    app.finish(Ok("saved".into()));
    assert_eq!(app.queued_action(), None);
}

#[test]
fn exit_takes_priority_over_queued_preferences_but_not_busy_or_finished_work() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    assert_eq!(app.queued_action(), None);
    app.preferences_dirty = true;
    app.begin(Action::CheckUpdate).unwrap();
    assert_eq!(app.begin(Action::Exit), None);
    assert_eq!(app.queued_action(), None);
    app.finish(Ok("checked".into()));
    assert_eq!(app.queued_action(), Some(Action::Exit));
    app.begin(app.queued_action().unwrap()).unwrap();
    assert_eq!(app.queued_action(), None);
    app.exit_ready = true;
    app.finish(Ok("exited".into()));
    assert_eq!(app.queued_action(), None);
}

#[test]
fn inspecting_a_certificate_revokes_stale_confirmation() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.ca_fingerprint = Some("old certificate".into());
    app.ca_keychain = Some("/test/login.keychain-db".into());
    app.ca_confirm = true;
    app.begin(Action::InspectCa).unwrap();
    app.finish(Err("certificate is invalid".into()));
    assert!(!app.can_trust_ca());
    assert_eq!(app.ca_fingerprint, None);
    assert_eq!(app.ca_keychain, None);
    assert_eq!(app.error.as_deref(), Some("certificate is invalid"));
}

#[test]
fn certificate_ownership_is_durable_and_cannot_discard_a_previously_recorded_ca() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.toml");
    let mut preferences = Preferences::default();
    preferences.record_ca("old fingerprint", &path).unwrap();
    assert_eq!(
        Preferences::load(&path)
            .unwrap()
            .trusted_fingerprint
            .as_deref(),
        Some("old fingerprint")
    );
    let before = fs_err::read(&path).unwrap();
    assert!(preferences.record_ca("new fingerprint", &path).is_err());
    assert_eq!(fs_err::read(&path).unwrap(), before);
    let blocked = dir.path().join("directory");
    fs_err::create_dir(&blocked).unwrap();
    assert!(
        Preferences::default()
            .record_ca("candidate", &blocked)
            .is_err()
    );
}

#[test]
fn exit_waits_for_pending_work_and_errors_keep_the_window_available() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.begin(Action::Start).unwrap();
    assert_eq!(app.begin(Action::Exit), None);
    assert!(app.exiting);
    assert!(!app.exit_ready);
    app.finish(Ok("started".into()));
    assert!(app.exiting);
    assert_eq!(app.begin(Action::Exit), Some(Action::Exit));
    app.finish(Err("system proxy restoration denied".into()));
    assert!(!app.exiting);
    assert!(!app.exit_ready);
    assert!(!app.busy);
    assert!(app.error.unwrap().contains("restoration denied"));
}

#[test]
fn certificate_confirmation_requires_a_resolved_target_keychain() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.ca_confirm = true;
    app.ca_fingerprint = Some("fingerprint".into());
    assert!(!app.can_trust_ca());
    app.ca_keychain = Some("/test/login.keychain-db".into());
    assert!(app.can_trust_ca());
}

#[test]
fn failed_trust_keeps_candidate_ownership_available_for_cleanup() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.ca_confirm = true;
    app.ca_fingerprint = Some("candidate".into());
    app.ca_keychain = Some("/test/login.keychain-db".into());
    app.begin(Action::TrustCa).unwrap();
    app.finish(Err("authorization cancelled".into()));
    assert_eq!(
        app.preferences.trusted_fingerprint.as_deref(),
        Some("candidate")
    );
}

#[test]
fn english_fallback_covers_every_translation_key_used_by_views() {
    let locale: serde_json::Value =
        serde_json::from_str(include_str!("../locales/app.yml")).unwrap();
    for source in [
        include_str!("../src/view/mod.rs"),
        include_str!("../src/view/dashboard.rs"),
        include_str!("../src/view/rules.rs"),
        include_str!("../src/view/logs.rs"),
        include_str!("../src/view/settings.rs"),
    ] {
        for key in source.split('"').skip(1).step_by(2).filter(|key| {
            key.contains('.')
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
        }) {
            assert!(
                locale[key]["en"].as_str().is_some(),
                "missing English key {key}"
            );
            assert!(
                locale[key]["zh-CN"].as_str().is_some(),
                "missing Chinese key {key}"
            );
        }
    }
}

#[tokio::test]
async fn preference_write_failure_is_reported_separately_from_committed_configuration() {
    use proxy_fork_core::{app_config::FileConfig, proxy_service::ProxyService};
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let preferences_path = dir.path().join("gui.toml");
    fs_err::create_dir(&preferences_path).unwrap();
    let service = ProxyService::new(dir.path().join("recovery.toml"));
    let mut config = AppConfig {
        enable_ca: false,
        ..AppConfig::default()
    };
    config.listen.port = 18798;
    let mut start = config.clone();
    start.listen.port = 0;
    service.start(start).await.unwrap();
    let result = proxy_fork_gui::preferences::save_configuration(
        &service,
        &config,
        config_path.clone(),
        Preferences::default(),
        preferences_path,
    )
    .await;
    assert!(
        result.is_ok(),
        "main configuration committed, not a total save failure"
    );
    assert!(result.unwrap().unwrap().contains("gui.toml"));
    assert_eq!(
        FileConfig::load(&config_path)
            .unwrap()
            .runtime()
            .unwrap()
            .listen
            .port,
        18798
    );
    assert!(matches!(
        *service.subscribe().borrow(),
        proxy_fork_core::proxy_service::ProxyServiceState::Running(_)
    ));
    service.stop().await.unwrap();
}
