use crate::{
    args::{GlobalConfigArgs, StartProxyArgs},
    dirs::user_proxy_fork_config_dir,
};
use anyhow::Result;
pub use proxy_fork_core::app_config::*;
use std::path::PathBuf;

pub fn load_start_proxy_config(
    global: &GlobalConfigArgs,
    args: &StartProxyArgs,
) -> Result<AppConfig> {
    let mut file = FileConfig::default();
    if let Some(path) = user_proxy_fork_config_dir()
        .map(|p| p.join("config.toml"))
        .filter(|p| p.exists())
        && let Ok(next) = FileConfig::load(&path)
    {
        file = merge_file_cfg(file, next);
    }
    if let Some(path) = ["proxy-fork.toml", "config.toml"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
        && let Ok(next) = FileConfig::load(&path)
    {
        file = merge_file_cfg(file, next);
    }
    if let Some(path) = global.config.as_ref().filter(|p| p.exists()) {
        file = merge_file_cfg(file, FileConfig::load(path)?);
    }
    let mut cfg = AppConfig::default();
    if let Some((host, port)) = args
        .listen
        .as_deref()
        .or(file.listen.as_deref())
        .and_then(split_host_port)
    {
        cfg.listen = ListenAddr { host, port };
    }
    cfg.ca_cert = args
        .ca_cert
        .clone()
        .or_else(|| file.cert.map(PathBuf::from));
    cfg.ca_key = args.ca_key.clone().or_else(|| file.key.map(PathBuf::from));
    cfg.enable_ca = !args.noca && !file.noca.unwrap_or(false);
    let section = file.proxy_manager.unwrap_or_default();
    cfg.proxy_manager.cache_size = section.cache_size.unwrap_or(1000);
    cfg.proxy_manager.rules = section.rules.unwrap_or_default();
    cfg.proxy_manager.rules.extend(args.rules.clone());
    cfg.proxy_manager.rules.retain(|rule| match rule.runtime() {
        Ok(_) => true,
        Err(error) => {
            tracing::error!(%error, "invalid CLI rule skipped");
            false
        }
    });
    cfg.enable_sysproxy = args.enable_sysproxy;
    cfg.debug = global.debug;
    Ok(cfg)
}

fn merge_file_cfg(mut base: FileConfig, other: FileConfig) -> FileConfig {
    if other.cert.is_some() {
        base.cert = other.cert;
    }
    if other.key.is_some() {
        base.key = other.key;
    }
    if other.listen.is_some() {
        base.listen = other.listen;
    }
    if other.noca.is_some() {
        base.noca = other.noca;
    }
    match (base.proxy_manager.take(), other.proxy_manager) {
        (Some(mut a), Some(b)) => {
            if b.cache_size.is_some() {
                a.cache_size = b.cache_size;
            }
            if b.rules.is_some() {
                a.rules = b.rules;
            }
            base.proxy_manager = Some(a);
        }
        (None, b) => base.proxy_manager = b,
        (a, None) => base.proxy_manager = a,
    }
    base
}
