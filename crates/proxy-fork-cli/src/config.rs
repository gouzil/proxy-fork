use std::path::{Path, PathBuf};

use derive_builder::Builder;
use fs_err as fs;
use serde::Deserialize;
use tracing::debug;

use crate::args::{GlobalConfigArgs, RuleItem, StartProxyArgs};
use crate::dirs::user_proxy_fork_config_dir;
use anyhow::Result;

/// 文件配置（TOML）
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FileConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    /// 监听地址（默认 127.0.0.1:7898）
    pub listen: Option<String>,
    /// 禁用 CA 证书（无证书模式）
    pub noca: Option<bool>,
    /// 代理规则
    pub proxy_manager: Option<ProxyManagerSection>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProxyManagerSection {
    /// 规则列表
    pub rules: Option<Vec<RuleItem>>,
    /// LRU 缓存大小
    pub cache_size: Option<usize>,
}

/// 运行时合并后的配置
#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned")]
pub struct AppConfig {
    #[builder(default)]
    pub ca_cert: Option<PathBuf>,
    #[builder(default)]
    pub ca_key: Option<PathBuf>,
    pub listen: ListenAddr,
    pub proxy_manager: ProxyManagerRuntime,
    #[builder(default = "false")]
    pub enable_sysproxy: bool,
    #[builder(default = "0")]
    pub debug: u8,
    #[builder(default = "true")]
    pub enable_ca: bool,
}

#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned")]
pub struct ProxyManagerRuntime {
    #[builder(default = "default_cache_size()")]
    pub cache_size: usize,
    #[builder(default)]
    pub rules: Vec<RuleItem>,
}

fn default_cache_size() -> usize {
    1000
}

/// 监听地址结构，支持在类型层声明默认值
#[derive(Debug, Clone, Builder)]
#[builder(pattern = "owned")]
pub struct ListenAddr {
    #[builder(default = "default_listen_host()")]
    pub host: String,
    #[builder(default = "default_listen_port()")]
    pub port: u16,
}

fn default_listen_host() -> String {
    "127.0.0.1".to_string()
}
fn default_listen_port() -> u16 {
    7898
}

/// 加载配置的入口：按优先级合并 CLI > CWD 文件 > 用户目录文件
pub fn load_start_proxy_config(
    global: &GlobalConfigArgs,
    start_args: &StartProxyArgs,
) -> Result<AppConfig> {
    // 1. 用户目录默认配置路径：~/.config/proxy-fork/config.toml（或平台对应路径）
    let user_cfg_path = user_proxy_fork_config_dir().map(|p| p.join("config.toml"));

    // 2. 当前目录配置文件 ./proxy-fork.toml 或 ./config.toml（择一，proxy-fork.toml 优先）
    let cwd_cfg_path = find_first_existing([
        PathBuf::from("proxy-fork.toml"),
        PathBuf::from("config.toml"),
    ]);

    // 3. 如果 CLI 指定 --config 则优先使用
    let cli_cfg_path = global.config.clone();

    // 依次读取（后读覆盖前读）
    let mut file_cfg = FileConfig::default();
    if let Some(p) = user_cfg_path.as_ref().filter(|p| p.exists()) {
        if let Ok(c) = read_toml_file(p) {
            file_cfg = merge_file_cfg(file_cfg, c);
        }
    }
    if let Some(p) = cwd_cfg_path.as_ref() {
        if let Ok(c) = read_toml_file(p) {
            file_cfg = merge_file_cfg(file_cfg, c);
        }
    }
    if let Some(p) = cli_cfg_path.as_ref() {
        if p.exists() {
            let c = read_toml_file(p)?;
            file_cfg = merge_file_cfg(file_cfg, c);
        }
    }

    // 构造运行时配置，应用 CLI 覆盖
    // 监听地址：优先 CLI > 文件；否则采用 ListenAddr 的默认
    let listen = start_args
        .listen
        .clone()
        .or_else(|| file_cfg.listen.clone());
    let listen = if let Some(s) = listen {
        let (host, port) =
            split_host_port(&s).unwrap_or((default_listen_host(), default_listen_port()));
        ListenAddrBuilder::default()
            .host(host)
            .port(port)
            .build()
            .unwrap()
    } else {
        ListenAddrBuilder::default().build().unwrap()
    };

    let ca_cert = start_args
        .ca_cert
        .clone()
        .or_else(|| file_cfg.cert.as_ref().map(PathBuf::from));
    let ca_key = start_args
        .ca_key
        .clone()
        .or_else(|| file_cfg.key.as_ref().map(PathBuf::from));

    // 计算是否启用 CA：CLI noca 或文件 noca 设为 true 时禁用
    let enable_ca = !start_args.noca && !file_cfg.noca.unwrap_or(false);

    let pm_section = file_cfg.proxy_manager.unwrap_or_default();
    // 合并规则：文件中的规则先加入，再追加 CLI 规则
    let mut rules = pm_section.rules.unwrap_or_default();
    if !start_args.rules.is_empty() {
        rules.extend(start_args.rules.clone().into_iter());
    }
    let proxy_manager = ProxyManagerRuntimeBuilder::default()
        .cache_size(pm_section.cache_size.unwrap_or_else(default_cache_size))
        .rules(rules)
        .build()
        .unwrap();

    Ok(AppConfigBuilder::default()
        .ca_cert(ca_cert)
        .ca_key(ca_key)
        .listen(listen)
        .proxy_manager(proxy_manager)
        .enable_sysproxy(start_args.enable_sysproxy)
        .debug(global.debug)
        .enable_ca(enable_ca)
        .build()
        .unwrap())
}

fn read_toml_file(path: &Path) -> Result<FileConfig> {
    let text = fs::read_to_string(path)?;
    let cfg: FileConfig = toml::from_str(&text)?;
    debug!("loaded config from {}", path.display());
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
        (None, x) => base.proxy_manager = x,
        (Some(mut a), Some(b)) => {
            if b.cache_size.is_some() {
                a.cache_size = b.cache_size;
            }
            if b.rules.is_some() {
                a.rules = b.rules;
            }
            base.proxy_manager = Some(a);
        }
        (Some(a), None) => base.proxy_manager = Some(a),
    }
    base
}

pub fn split_host_port(s: &str) -> Option<(String, u16)> {
    let (host, port_str) = s.rsplit_once(':')?;
    let port = port_str.parse().ok()?;
    Some((host.to_string(), port))
}

fn find_first_existing<I>(candidates: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    for p in candidates {
        if p.exists() {
            return Some(p);
        }
    }
    None
}
