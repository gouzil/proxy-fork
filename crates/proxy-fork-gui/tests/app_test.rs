use proxy_fork_core::app_config::{AppConfig, RuleItem};
use proxy_fork_gui::{
    app::{Action, App, Page},
    preferences::{Preferences, ThemePreference},
};

#[test]
fn tray_dashboard_navigation_returns_to_dashboard() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.page = Page::Logs;
    app.show_dashboard();
    assert_eq!(app.page, Page::Dashboard);
}

#[test]
fn tray_system_proxy_toggle_queues_a_config_save_when_idle() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    assert!(!app.config.enable_sysproxy);
    assert!(app.toggle_system_proxy());
    assert!(app.config.enable_sysproxy);
    assert_eq!(app.queued_action(), Some(Action::Save));
}

#[test]
fn tray_system_proxy_toggle_is_ignored_while_busy() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.begin(Action::Stop).unwrap();
    assert!(!app.toggle_system_proxy());
    assert!(!app.config.enable_sysproxy);
    assert_eq!(app.queued_action(), None);
}

#[test]
fn exit_flushes_overview_settings_without_a_save_button() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.listen_port = "18080".into();
    assert_eq!(app.begin(Action::Exit), Some(Action::Save));
    assert!(app.exiting);
    app.finish(Err("disk is read-only".into()));
    assert!(!app.exiting);
    assert_eq!(app.listen_port, "18080");
    assert_eq!(app.queued_action(), None, "failed writes must not loop");
}

#[test]
fn overview_autosave_does_not_commit_unfinished_rules_or_duplicate_writes() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.request_config_save();
    assert_eq!(app.queued_action(), None);
    app.add_rule(); // A new, unfinished rule must not prevent saving the listener.
    app.listen_port = "18080".into();
    assert!(app.edited_config().is_err());
    app.request_config_save();
    assert_eq!(app.queued_action(), Some(Action::Save));
    let snapshot = app.edited_settings().unwrap();
    assert!(snapshot.proxy_manager.rules.is_empty());
    assert_eq!(snapshot.listen.port, 18080);
    app.begin(Action::Save).unwrap();
    assert_eq!(app.queued_action(), None);
    app.config_saved(snapshot);
    app.finish(Ok("saved".into()));
    app.request_config_save();
    assert_eq!(app.queued_action(), None);
    assert_eq!(app.rules.len(), 1);
}

#[test]
fn overview_changes_during_a_save_keep_the_latest_draft_and_preserve_other_errors() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.begin(Action::Stop).unwrap();
    app.listen_port = "18080".into();
    app.request_config_save();
    app.finish(Err("system proxy restoration denied".into()));
    assert_eq!(app.queued_action(), Some(Action::Save));
    app.begin(Action::Save).unwrap();
    let snapshot = app.edited_settings().unwrap();
    app.listen_port = "7898".into(); // Revert while the first snapshot is being saved.
    app.request_config_save();
    app.config_saved(snapshot);
    app.finish(Ok("saved".into()));
    assert_eq!(app.listen_port, "7898");
    assert_eq!(app.queued_action(), Some(Action::Save));
    app.begin(Action::Save).unwrap();
    app.config_saved(app.edited_settings().unwrap());
    app.finish(Ok("saved".into()));
    assert_eq!(app.queued_action(), None);
    assert_eq!(
        app.error.as_deref(),
        Some("system proxy restoration denied")
    );
}

#[test]
fn invalid_overview_edits_remain_visible_without_retry_loops_or_blocking_rule_saves() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.listen_port = "0".into();
    app.request_config_save();
    app.begin(app.queued_action().unwrap()).unwrap();
    app.finish(Err(app.edited_settings().unwrap_err().to_string()));
    assert!(
        app.config_save_error
            .as_deref()
            .unwrap()
            .contains("non-zero")
    );
    assert_eq!(app.queued_action(), None);
    app.add_rule();
    app.rules[0].rule.host = "saved.example".into();
    app.request_rules_save();
    app.begin(Action::SaveRules).unwrap();
    app.rules_saved(app.edited_rules().unwrap());
    app.finish(Ok("saved".into()));
    assert_eq!(app.queued_action(), None);
    assert!(app.config_save_error.is_some());
    app.listen_port = "7898".into();
    app.request_config_save();
    assert_eq!(app.config_save_error, None);
    assert_eq!(
        app.queued_action(),
        None,
        "rule saves must update the comparison baseline"
    );
}

#[test]
fn exit_flushes_both_rules_and_settings_in_sequence() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "saved.example".into();
    app.listen_port = "18080".into();
    assert_eq!(app.begin(Action::Exit), Some(Action::SaveRules));
    app.rules_saved(app.edited_rules().unwrap());
    app.finish(Ok("rules saved".into()));
    assert_eq!(app.queued_action(), Some(Action::Save));
    app.begin(Action::Save).unwrap();
    app.config_saved(app.edited_settings().unwrap());
    app.finish(Ok("settings saved".into()));
    assert_eq!(app.queued_action(), Some(Action::Exit));
    assert_eq!(app.begin(Action::Exit), Some(Action::Exit));
}

#[test]
fn leaving_overview_mid_edit_keeps_the_restart_warning_and_queues_the_save() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.state = proxy_fork_core::proxy_service::ProxyServiceState::Running(
        "127.0.0.1:7898".parse().unwrap(),
    );
    app.begin(Action::CheckUpdate).unwrap();
    app.listen_port = "18080".into();
    app.request_config_save(); // The sidebar/hide path, without another Dashboard frame.
    assert!(app.restart_required);
    assert_eq!(app.queued_action(), None);
    app.finish(Ok("checked".into()));
    assert_eq!(app.queued_action(), Some(Action::Save));
}

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
fn rule_autosave_is_independent_of_other_drafts_and_tracks_committed_rules() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.request_rules_save();
    assert_eq!(app.queued_action(), None);
    app.add_rule();
    app.rules[0].rule.host = "first.example".into();
    app.listen_port = "invalid".into();
    app.cache_size = "invalid".into();
    app.request_rules_save();
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    assert!(app.edited_config().is_err());
    let saved = app.edited_rules().unwrap();
    app.begin(Action::SaveRules).unwrap();
    assert_eq!(app.queued_action(), None);
    assert!(app.config.proxy_manager.rules.is_empty());

    app.rules[0].rule.host = "second.example".into();
    app.request_rules_save();
    app.rules_saved(saved.clone());
    app.finish(Ok("saved".into()));
    assert_eq!(app.config.proxy_manager.rules, saved);
    assert_eq!(app.rules[0].rule.host, "second.example");
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    app.begin(Action::SaveRules).unwrap();
    let saved = app.edited_rules().unwrap();
    app.request_rules_save(); // Closing during the write must not duplicate it.
    app.rules_saved(saved);
    app.finish(Ok("saved".into()));
    app.request_rules_save();
    assert_eq!(app.queued_action(), None);
}

#[test]
fn invalid_or_failed_rule_autosave_keeps_drafts_without_retrying_forever() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.request_rules_save();
    app.begin(app.queued_action().unwrap()).unwrap();
    let error = app.edited_rules().unwrap_err().to_string();
    app.finish(Err(error.clone()));
    assert!(app.config.proxy_manager.rules.is_empty());
    assert_eq!(app.rules.len(), 1);
    assert_eq!(app.rule_save_error.as_deref(), Some(error.as_str()));
    assert_eq!(app.queued_action(), None);

    app.rules[0].rule.host = "fixed.example".into();
    app.begin(Action::CheckUpdate).unwrap();
    app.request_rules_save();
    assert_eq!(app.queued_action(), None);
    app.finish(Ok("checked".into()));
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    app.begin(app.queued_action().unwrap()).unwrap();
    app.finish(Err("disk is read-only".into()));
    assert_eq!(app.queued_action(), None);
    assert!(app.config.proxy_manager.rules.is_empty());
    assert_eq!(app.rules[0].rule.host, "fixed.example");
}

#[test]
fn reverting_a_rule_while_its_save_is_in_flight_is_not_lost() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "original.example".into();
    app.rules_saved(app.edited_rules().unwrap());
    app.rules[0].rule.host = "in-flight.example".into();
    app.request_rules_save();
    app.begin(Action::SaveRules).unwrap();
    let saved = app.edited_rules().unwrap();
    app.rules[0].rule.host = "original.example".into();
    app.request_rules_save();
    app.rules_saved(saved);
    app.finish(Ok("saved".into()));
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    assert_eq!(app.edited_rules().unwrap()[0].host, "original.example");
}

#[test]
fn exit_flushes_rules_before_stopping_and_failed_save_cancels_exit() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "last.example".into();
    app.rule_editor = Some(app.rules[0].id);
    app.begin(Action::CheckUpdate).unwrap();
    assert_eq!(app.begin(Action::Exit), None);
    app.finish(Ok("checked".into()));
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    app.begin(app.queued_action().unwrap()).unwrap();
    app.rules_saved(app.edited_rules().unwrap());
    app.finish(Ok("saved".into()));
    assert_eq!(app.queued_action(), Some(Action::Exit));
    assert_eq!(app.begin(Action::Exit), Some(Action::Exit));
    app.finish(Ok("exited".into()));

    app.rules[0].rule.host.clear();
    assert_eq!(app.begin(Action::Exit), Some(Action::SaveRules));
    app.finish(Err("invalid rule".into()));
    assert!(!app.exiting);
    assert_eq!(app.queued_action(), None);
}

#[test]
fn failed_rule_saves_are_not_discarded_by_exit_after_the_editor_is_closed() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "unsaved.example".into();
    app.request_rules_save();
    app.begin(Action::SaveRules).unwrap();
    app.finish(Err("disk is read-only".into()));
    assert_eq!(app.rule_editor, None);
    assert_eq!(app.queued_action(), None, "no automatic retry loop");
    assert_eq!(app.begin(Action::Exit), Some(Action::SaveRules));
    app.finish(Err("disk is still read-only".into()));
    assert!(!app.exiting);
    assert_eq!(app.queued_action(), None);
    assert_eq!(app.rules[0].rule.host, "unsaved.example");
}

#[test]
fn rule_autosave_does_not_hide_the_failure_of_the_operation_it_waited_for() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "queued.example".into();
    app.begin(Action::Stop).unwrap();
    app.request_rules_save();
    app.finish(Err("system proxy restoration denied".into()));
    app.begin(app.queued_action().unwrap()).unwrap();
    assert_eq!(
        app.error.as_deref(),
        Some("system proxy restoration denied")
    );
    app.rules_saved(app.edited_rules().unwrap());
    app.finish(Ok("saved".into()));
    assert_eq!(
        app.error.as_deref(),
        Some("system proxy restoration denied")
    );
    assert_eq!(app.rule_save_error, None);

    app.rules[0].rule.host = "retry.example".into();
    app.request_rules_save();
    app.begin(app.queued_action().unwrap()).unwrap();
    app.finish(Err("disk is read-only".into()));
    assert_eq!(
        app.error.as_deref(),
        Some("system proxy restoration denied")
    );
    assert_eq!(app.rule_save_error.as_deref(), Some("disk is read-only"));
    app.request_rules_save();
    app.begin(app.queued_action().unwrap()).unwrap();
    app.rules_saved(app.edited_rules().unwrap());
    app.finish(Ok("saved".into()));
    assert_eq!(app.rule_save_error, None);
    assert_eq!(
        app.error.as_deref(),
        Some("system proxy restoration denied")
    );
}

#[test]
fn restoring_the_saved_rule_clears_stale_errors_without_another_write() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "saved.example".into();
    app.rules_saved(app.edited_rules().unwrap());
    app.rules[0].source_port = "0".into();
    app.request_rules_save();
    app.begin(app.queued_action().unwrap()).unwrap();
    app.finish(Err(app.edited_rules().unwrap_err().to_string()));
    assert!(app.rule_save_error.is_some());
    app.rules[0].source_port.clear();
    app.request_rules_save();
    assert_eq!(app.queued_action(), None);
    assert_eq!(app.rule_save_error, None);
}

#[test]
fn loaded_empty_paths_are_not_normalized_by_autosave_or_exit() {
    let mut config = AppConfig::default();
    let original = RuleItem {
        protocol: "http".into(),
        host: "literal-empty.example".into(),
        path: Some(String::new()),
        target_host: "127.0.0.1".into(),
        path_transform: Some("replace".into()),
        target_path: Some(String::new()),
        ..RuleItem::default()
    };
    config.proxy_manager.rules.push(original.clone());
    let mut app = App::new(config.clone(), Preferences::default());
    app.request_rules_save();
    assert_eq!(app.queued_action(), None);
    assert_eq!(app.begin(Action::Exit), Some(Action::Exit));
    let mut app = App::new(config, Preferences::default());
    app.add_rule();
    app.rules[1].rule.host = "another.example".into();
    app.request_rules_save();
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    assert_eq!(app.edited_rules().unwrap()[0], original);
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
