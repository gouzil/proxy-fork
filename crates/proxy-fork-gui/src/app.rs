use crate::preferences::Preferences;
use anyhow::{Context, Result};
use proxy_fork_core::{
    app_config::{AppConfig, RuleItem},
    proxy_service::ProxyServiceState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Start,
    Stop,
    Save,
    SavePreferences,
    GenerateCa,
    InspectCa,
    TrustCa,
    RemoveCa,
    EnableAutostart,
    DisableAutostart,
    InspectAutostart,
    CheckUpdate,
    DownloadUpdate,
    InstallUpdate,
    Exit,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Rules,
    Logs,
    Settings,
}

pub struct RuleDraft {
    pub id: u64,
    pub rule: RuleItem,
    pub source_port: String,
    pub target_port: String,
}
impl RuleDraft {
    fn new(id: u64, rule: RuleItem) -> Self {
        Self {
            id,
            source_port: rule.port.map(|p| p.to_string()).unwrap_or_default(),
            target_port: rule.target_port.map(|p| p.to_string()).unwrap_or_default(),
            rule,
        }
    }
    fn parsed(&self) -> Result<RuleItem> {
        let mut rule = self.rule.clone();
        rule.port = if self.source_port.trim().is_empty() {
            None
        } else {
            Some(self.source_port.parse().context("invalid source port")?)
        };
        rule.target_port = if self.target_port.trim().is_empty() {
            None
        } else {
            Some(self.target_port.parse().context("invalid target port")?)
        };
        for value in [
            &mut rule.path,
            &mut rule.target_path,
            &mut rule.path_transform,
            &mut rule.target_protocol,
        ] {
            if value.as_ref().is_some_and(|s| s.is_empty()) {
                *value = None;
            }
        }
        rule.runtime()?;
        Ok(rule)
    }
}

pub struct App {
    pub config: AppConfig,
    pub preferences: Preferences,
    pub preferences_dirty: bool,
    pub page: Page,
    pub state: ProxyServiceState,
    pub busy: bool,
    pub exiting: bool,
    pub exit_ready: bool,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub rules: Vec<RuleDraft>,
    pub rule_editor: Option<u64>,
    pub listen_port: String,
    pub cache_size: String,
    pub cert_path: String,
    pub key_path: String,
    pub restart_required: bool,
    pub ca_dialog: bool,
    pub ca_confirm: bool,
    pub ca_overwrite: bool,
    pub ca_fingerprint: Option<String>,
    pub ca_keychain: Option<String>,
    pub ca_trusted: Option<bool>,
    pub ca_missing: bool,
    pub autostart: Option<bool>,
    pub update_dialog: bool,
    pub update_version: Option<String>,
    pub update_notes: String,
    pub update_downloaded: bool,
    pub update_progress: (u64, Option<u64>),
    pub auto_scroll: bool,
    pub log_level: usize,
    pub log_detail: Option<String>,
    next_rule_id: u64,
}
impl App {
    pub fn new(mut config: AppConfig, preferences: Preferences) -> Self {
        config.enable_sysproxy = preferences.system_proxy;
        let rules = config
            .proxy_manager
            .rules
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, r)| RuleDraft::new(i as u64, r))
            .collect();
        let next_rule_id = config.proxy_manager.rules.len() as u64;
        Self {
            listen_port: config.listen.port.to_string(),
            cache_size: config.proxy_manager.cache_size.to_string(),
            cert_path: config
                .ca_cert
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            key_path: config
                .ca_key
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            config,
            preferences,
            preferences_dirty: false,
            page: Page::Dashboard,
            state: ProxyServiceState::Stopped,
            busy: false,
            exiting: false,
            exit_ready: false,
            notice: None,
            error: None,
            rules,
            rule_editor: None,
            next_rule_id,
            restart_required: false,
            ca_dialog: false,
            ca_confirm: false,
            ca_overwrite: false,
            ca_fingerprint: None,
            ca_keychain: None,
            ca_trusted: None,
            ca_missing: false,
            autostart: None,
            update_dialog: false,
            update_version: None,
            update_notes: String::new(),
            update_downloaded: false,
            update_progress: (0, None),
            auto_scroll: true,
            log_level: 0,
            log_detail: None,
        }
    }
    pub fn set_locale(&mut self, locale: &str) {
        if matches!(locale, "en" | "zh-CN") {
            self.preferences.locale = locale.into();
            rust_i18n::set_locale(locale);
        }
    }
    pub fn add_rule(&mut self) {
        self.rules.push(RuleDraft::new(
            self.next_rule_id,
            RuleItem {
                protocol: "https".into(),
                host: String::new(),
                target_host: "127.0.0.1".into(),
                ..RuleItem::default()
            },
        ));
        self.next_rule_id += 1;
    }
    pub fn edited_config(&self) -> Result<AppConfig> {
        let mut cfg = self.config.clone();
        cfg.listen.port = self.listen_port.parse().context("invalid listen port")?;
        anyhow::ensure!(cfg.listen.port > 0, "listen port must be non-zero");
        cfg.proxy_manager.cache_size = self.cache_size.parse().context("invalid cache size")?;
        cfg.proxy_manager.rules = self
            .rules
            .iter()
            .map(RuleDraft::parsed)
            .collect::<Result<_>>()?;
        cfg.ca_cert = if self.cert_path.is_empty() {
            None
        } else {
            Some(self.cert_path.clone().into())
        };
        cfg.ca_key = if self.key_path.is_empty() {
            None
        } else {
            Some(self.key_path.clone().into())
        };
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn queued_action(&self) -> Option<Action> {
        if self.busy || self.exit_ready {
            None
        } else if self.exiting {
            Some(Action::Exit)
        } else if self.preferences_dirty {
            Some(Action::SavePreferences)
        } else {
            None
        }
    }
    pub fn begin(&mut self, action: Action) -> Option<Action> {
        if action == Action::Exit {
            self.exiting = true;
        }
        if self.busy {
            return None;
        }
        self.busy = true;
        if action != Action::SavePreferences || !self.preferences_dirty {
            self.error = None;
        }
        if action == Action::SavePreferences {
            self.preferences_dirty = false;
        }
        self.notice = None;
        if action == Action::InspectCa {
            self.ca_fingerprint = None;
            self.ca_keychain = None;
            self.ca_trusted = None;
            self.ca_confirm = false;
            self.ca_missing = false;
        }
        if action == Action::TrustCa
            && self.can_trust_ca()
            && self.preferences.trusted_fingerprint.is_none()
        {
            self.preferences.trusted_fingerprint = self.ca_fingerprint.clone();
        }
        Some(action)
    }
    pub fn finish(&mut self, result: Result<String, String>) {
        self.busy = false;
        match result {
            Ok(message) => self.notice = Some(message),
            Err(error) => {
                self.error = Some(match self.error.take() {
                    Some(previous) => format!("{previous}\n{error}"),
                    None => error,
                });
                self.exiting = false;
            }
        }
    }
    pub fn running(&self) -> bool {
        matches!(self.state, ProxyServiceState::Running(_))
    }
    pub fn can_trust_ca(&self) -> bool {
        self.ca_confirm && self.ca_fingerprint.is_some() && self.ca_keychain.is_some()
    }
    pub fn ca_not_configured(&mut self, recorded: Option<String>) {
        self.ca_fingerprint = None;
        self.ca_keychain = None;
        self.ca_trusted = None;
        self.ca_missing = true;
        self.ca_confirm = false;
        self.ca_overwrite = false;
        self.preferences.trusted_fingerprint = recorded;
    }
}
