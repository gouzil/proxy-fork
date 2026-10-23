use etcetera::BaseStrategy;
use std::path::PathBuf;

pub const APP_NAME: &str = "proxy-fork";

pub fn user_cache_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .ok()
        .map(|d| d.cache_dir().join(APP_NAME))
}
pub fn user_state_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .ok()
        .map(|d| d.data_dir().join(APP_NAME))
}
pub fn user_config_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .ok()
        .map(|d| d.config_dir())
}
pub fn user_proxy_fork_config_dir() -> Option<PathBuf> {
    user_config_dir().map(|p| p.join(APP_NAME))
}
pub fn default_cert_path() -> Option<PathBuf> {
    user_state_dir().map(|p| p.join(format!("{APP_NAME}-ca-cert.pem")))
}
pub fn default_private_key_path() -> Option<PathBuf> {
    user_state_dir().map(|p| p.join(format!("{APP_NAME}-ca.pem")))
}
