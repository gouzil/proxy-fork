use std::path::PathBuf;

use etcetera::BaseStrategy;

pub const APP_NAME: &str = "proxy-fork";

/// Returns an appropriate user-level directory for storing the cache.
///
/// Corresponds to `$XDG_CACHE_HOME/proxy-fork` on Unix.
pub fn user_cache_dir() -> Option<PathBuf> {
    etcetera::base_strategy::choose_base_strategy()
        .ok()
        .map(|dirs| dirs.cache_dir().join(APP_NAME))
}

/// Returns an appropriate user-level directory for storing application state.
///
/// Corresponds to `$XDG_DATA_HOME/proxy-fork` on Unix.
pub fn user_state_dir() -> Option<PathBuf> {
    etcetera::base_strategy::choose_base_strategy()
        .ok()
        .map(|dirs| dirs.data_dir().join(APP_NAME))
}

/// Returns the path to the user configuration directory.
///
/// On Windows, use, e.g., C:\Users\<username>\AppData\Roaming
/// On Linux and macOS, use `XDG_CONFIG_HOME` or $HOME/.config, e.g., /home/<username>/.config.
pub fn user_config_dir() -> Option<PathBuf> {
    etcetera::choose_base_strategy()
        .map(|dirs| dirs.config_dir())
        .ok()
}

pub fn user_proxy_fork_config_dir() -> Option<PathBuf> {
    user_config_dir().map(|mut path| {
        path.push(APP_NAME);
        path
    })
}

// 获取默认的证书路径
pub fn default_cert_path() -> Option<PathBuf> {
    user_state_dir().map(|mut path| {
        path.push(format!("{APP_NAME}-ca-cert.pem"));
        path
    })
}

// 获取默认的私钥路径
pub fn default_private_key_path() -> Option<PathBuf> {
    user_state_dir().map(|mut path| {
        path.push(format!("{APP_NAME}-ca.pem"));
        path
    })
}
