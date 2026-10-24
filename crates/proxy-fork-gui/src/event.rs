use crate::{
    app::Action,
    updater::{Release, VerifiedUpdate},
};
use proxy_fork_core::{
    GeneratedCa,
    app_config::{AppConfig, RuleItem},
    proxy_service::ProxyServiceState,
};

pub enum Outcome {
    Done(&'static str),
    Saved(Option<String>, AppConfig),
    RulesSaved(Vec<RuleItem>),
    Preferences,
    Certificate(GeneratedCa, Option<String>),
    CaNotConfigured(Option<String>),
    Trust {
        fingerprint: String,
        trusted: bool,
        recorded: Option<String>,
        keychain: Option<String>,
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
    DownloadProgress(u64, Option<u64>),
}
