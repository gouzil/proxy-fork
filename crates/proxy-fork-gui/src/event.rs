use crate::{
    app::Action,
    updater::{Release, VerifiedUpdate},
};
use proxy_fork_core::{
    CertificateInfo, GeneratedCa,
    app_config::{AppConfig, RuleItem},
    proxy_service::ProxyServiceState,
};

pub enum Outcome {
    Done(&'static str),
    Saved(Option<String>, AppConfig),
    RulesSaved(Vec<RuleItem>),
    Preferences,
    Certificate(GeneratedCa, Option<String>, CertificateInfo),
    CaNotConfigured(Option<String>),
    Trust {
        notice: &'static str,
        fingerprint: String,
        trusted: bool,
        recorded: Option<String>,
        keychain: Option<String>,
        info: Option<CertificateInfo>,
    },
    Autostart(bool),
    Update(Option<Release>),
    Downloaded(VerifiedUpdate),
    Exit,
    ManualUpdate(std::path::PathBuf),
}
pub enum AppEvent {
    Action(Action),
    Finished(Result<Outcome, String>),
    State(ProxyServiceState),
    LogsReady,
    Show,
    ShowDashboard,
    ToggleSystemProxy,
    DownloadProgress(u64, Option<u64>),
}
