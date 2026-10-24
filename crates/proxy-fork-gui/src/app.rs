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
    SaveRules,
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
    pub(crate) optional_literal_empty: bool,
}
impl RuleDraft {
    fn new(id: u64, rule: RuleItem) -> Self {
        Self {
            id,
            source_port: rule.port.map(|p| p.to_string()).unwrap_or_default(),
            target_port: rule.target_port.map(|p| p.to_string()).unwrap_or_default(),
            optional_literal_empty: false,
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
    pub rule_save_error: Option<String>,
    pub config_save_error: Option<String>,
    pub config_dirty: bool,
    pub rules: Vec<RuleDraft>,
    pub rules_dirty: bool,
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
    pub log_filter: String,
    pub log_newest_first: bool,
    pub paused_logs: Option<std::collections::VecDeque<crate::logging::LogRecord>>,
    next_rule_id: u64,
    saving_rules: bool,
    saving_config: bool,
    saved_config: AppConfig,
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
            saved_config: config.clone(),
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
            rule_save_error: None,
            config_save_error: None,
            config_dirty: false,
            rules,
            rules_dirty: false,
            rule_editor: None,
            next_rule_id,
            saving_rules: false,
            saving_config: false,
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
            log_filter: String::new(),
            log_newest_first: false,
            paused_logs: None,
        }
    }
    pub fn show_dashboard(&mut self) {
        self.page = Page::Dashboard;
    }
    pub fn toggle_system_proxy(&mut self) -> bool {
        if self.busy {
            return false;
        }
        self.config.enable_sysproxy = !self.config.enable_sysproxy;
        self.request_config_save();
        true
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
        let mut cfg = self.edited_settings()?;
        cfg.proxy_manager.rules = self.edited_rules()?;
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn edited_settings(&self) -> Result<AppConfig> {
        let mut cfg = self.config.clone();
        cfg.listen.port = self.listen_port.parse().context("invalid listen port")?;
        anyhow::ensure!(cfg.listen.port > 0, "listen port must be non-zero");
        cfg.proxy_manager.cache_size = self.cache_size.parse().context("invalid cache size")?;
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
    pub fn request_config_save(&mut self) {
        self.config_dirty = self.saving_config
            || self
                .edited_settings()
                .map_or(true, |config| config != self.saved_config);
        if !self.config_dirty {
            self.config_save_error = None;
        } else if self.running() {
            self.restart_required = true;
        }
    }
    pub fn config_saved(&mut self, config: AppConfig) {
        self.saved_config = config;
        self.config_save_error = None;
        if self.config_dirty {
            self.config_dirty = self
                .edited_settings()
                .map_or(true, |config| config != self.saved_config);
        }
    }
    pub fn edited_rules(&self) -> Result<Vec<RuleItem>> {
        self.rules
            .iter()
            .enumerate()
            .map(|(i, draft)| {
                draft
                    .parsed()
                    .with_context(|| format!("invalid rule {}", i + 1))
            })
            .collect()
    }
    pub fn request_rules_save(&mut self) {
        self.rules_dirty = self.saving_rules
            || self
                .edited_rules()
                .map_or(true, |rules| rules != self.config.proxy_manager.rules);
        if !self.rules_dirty {
            self.rule_save_error = None;
        }
    }
    pub fn rules_saved(&mut self, rules: Vec<RuleItem>) {
        self.saved_config.proxy_manager.rules = rules.clone();
        self.config.proxy_manager.rules = rules;
        self.rule_save_error = None;
        if self.rules_dirty {
            self.rules_dirty = self
                .edited_rules()
                .map_or(true, |rules| rules != self.config.proxy_manager.rules);
        }
    }
    pub fn queued_action(&self) -> Option<Action> {
        if self.busy || self.exit_ready {
            None
        } else if self.rules_dirty {
            Some(Action::SaveRules)
        } else if self.config_dirty {
            Some(Action::Save)
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
            self.request_rules_save();
            self.request_config_save();
        }
        if self.busy {
            return None;
        }
        let action = if action == Action::Exit && self.rules_dirty {
            Action::SaveRules
        } else if action == Action::Exit && self.config_dirty {
            Action::Save
        } else {
            action
        };
        self.busy = true;
        self.saving_rules = action == Action::SaveRules;
        self.saving_config = action == Action::Save;
        if self.saving_rules {
            self.rule_save_error = None;
        } else if self.saving_config {
            self.config_save_error = None;
        } else if action != Action::SavePreferences || !self.preferences_dirty {
            self.error = None;
        }
        if action == Action::SavePreferences {
            self.preferences_dirty = false;
        }
        if action == Action::SaveRules {
            self.rules_dirty = false;
        }
        if action == Action::Save {
            self.config_dirty = false;
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
        let saving_rules = std::mem::take(&mut self.saving_rules);
        let saving_config = std::mem::take(&mut self.saving_config);
        self.busy = false;
        if self.rules_dirty {
            self.request_rules_save();
        }
        if self.config_dirty {
            self.request_config_save();
        }
        match result {
            Ok(message) => self.notice = Some(message),
            Err(error) => {
                if saving_rules {
                    self.rule_save_error = Some(error);
                } else if saving_config {
                    self.config_save_error = Some(error);
                } else {
                    self.error = Some(match self.error.take() {
                        Some(previous) => format!("{previous}\n{error}"),
                        None => error,
                    });
                }
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
