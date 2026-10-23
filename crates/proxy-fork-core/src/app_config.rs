use crate::{Address, AddressBuilder, AddressPattern, PathTransformMode, Protocol, ProxyManager};
use anyhow::{Context, Result, bail};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct FileConfig {
    pub cert: Option<String>,
    pub key: Option<String>,
    pub listen: Option<String>,
    pub noca: Option<bool>,
    pub proxy_manager: Option<ProxyManagerSection>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ProxyManagerSection {
    pub rules: Option<Vec<RuleItem>>,
    pub cache_size: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct RuleItem {
    pub protocol: String,
    pub host: String,
    pub path: Option<String>,
    pub port: Option<u16>,
    pub target_protocol: Option<String>,
    pub target_host: String,
    pub target_port: Option<u16>,
    pub path_transform: Option<String>,
    pub target_path: Option<String>,
}

#[derive(Debug, Clone, Builder, PartialEq)]
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
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ca_cert: None,
            ca_key: None,
            listen: ListenAddr::default(),
            proxy_manager: ProxyManagerRuntime::default(),
            enable_sysproxy: false,
            debug: 0,
            enable_ca: true,
        }
    }
}

#[derive(Debug, Clone, Builder, PartialEq)]
#[builder(pattern = "owned")]
pub struct ProxyManagerRuntime {
    #[builder(default = "1000")]
    pub cache_size: usize,
    #[builder(default)]
    pub rules: Vec<RuleItem>,
}
impl Default for ProxyManagerRuntime {
    fn default() -> Self {
        Self {
            cache_size: 1000,
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Builder, PartialEq)]
#[builder(pattern = "owned")]
pub struct ListenAddr {
    #[builder(default = "String::from(\"127.0.0.1\")")]
    pub host: String,
    #[builder(default = "7898")]
    pub port: u16,
}
impl Default for ListenAddr {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 7898,
        }
    }
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs_err::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    #[cfg(unix)]
    let directory = fs_err::File::open(parent)?;
    temp.persist(path).map_err(|e| e.error)?;
    // Rename commits the visible file; a later durability error must not claim it was unchanged.
    #[cfg(unix)]
    if let Err(error) = directory.sync_all() {
        tracing::warn!(%error, path = %path.display(), "File committed; directory durability could not be confirmed");
    }
    Ok(())
}

pub fn split_host_port(s: &str) -> Option<(String, u16)> {
    let (host, port) = s.rsplit_once(':')?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if host.is_empty() {
        return None;
    }
    Some((host.into(), port.parse().ok()?))
}

impl FileConfig {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(toml::from_str(&fs_err::read_to_string(path)?)?)
    }
    pub fn runtime(&self) -> Result<AppConfig> {
        let mut cfg = AppConfig::default();
        if let Some(listen) = &self.listen {
            let (host, port) =
                split_host_port(listen).context("invalid listen address (expected host:port)")?;
            cfg.listen = ListenAddr { host, port };
        }
        cfg.ca_cert = self.cert.as_ref().map(PathBuf::from);
        cfg.ca_key = self.key.as_ref().map(PathBuf::from);
        cfg.enable_ca = !self.noca.unwrap_or(false);
        if let Some(pm) = &self.proxy_manager {
            cfg.proxy_manager.cache_size = pm.cache_size.unwrap_or(1000);
            cfg.proxy_manager.rules = pm.rules.clone().unwrap_or_default();
        }
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let cfg = self.runtime()?;
        if cfg.enable_ca {
            let cert = cfg
                .ca_cert
                .or_else(crate::dirs::default_cert_path)
                .context("no certificate directory")?;
            let key = cfg
                .ca_key
                .or_else(crate::dirs::default_private_key_path)
                .context("no private key directory")?;
            anyhow::ensure!(cert.is_file() && key.is_file(), "CA files are missing");
        }
        atomic_write(path, toml::to_string_pretty(self)?.as_bytes())
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<()> {
        if self.listen.host.trim().is_empty() {
            bail!("listen host cannot be empty");
        }
        if self.proxy_manager.cache_size == 0 {
            bail!("cache_size must be non-zero");
        }
        for (i, rule) in self.proxy_manager.rules.iter().enumerate() {
            rule.runtime()
                .with_context(|| format!("invalid rule {}", i + 1))?;
        }
        Ok(())
    }
    pub async fn build_manager(&self) -> Result<ProxyManager> {
        self.validate()?;
        let mut manager = ProxyManager::from_config(
            ProxyManager::builder()
                .cache_size(self.proxy_manager.cache_size)
                .build()?,
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        for rule in &self.proxy_manager.rules {
            let (pattern, target) = rule.runtime()?;
            manager.add_rule(pattern, target).await;
        }
        Ok(manager)
    }
    pub fn file_config(&self) -> FileConfig {
        let host = if self.listen.host.contains(':') {
            format!("[{}]", self.listen.host)
        } else {
            self.listen.host.clone()
        };
        FileConfig {
            cert: self.ca_cert.as_ref().map(|p| p.display().to_string()),
            key: self.ca_key.as_ref().map(|p| p.display().to_string()),
            listen: Some(format!("{host}:{}", self.listen.port)),
            noca: Some(!self.enable_ca),
            proxy_manager: Some(ProxyManagerSection {
                rules: Some(self.proxy_manager.rules.clone()),
                cache_size: Some(self.proxy_manager.cache_size),
            }),
        }
    }
}

impl RuleItem {
    pub fn runtime(&self) -> Result<(AddressPattern, Address)> {
        fn protocol(s: &str) -> Result<Protocol> {
            match s.trim().to_ascii_lowercase().as_str() {
                "http" => Ok(Protocol::Http),
                "https" => Ok(Protocol::Https),
                _ => bail!("protocol must be http or https"),
            }
        }
        if self.host.trim().is_empty() || self.target_host.trim().is_empty() {
            bail!("rule host cannot be empty");
        }
        if self.port == Some(0) || self.target_port == Some(0) {
            bail!("rule port must be non-zero");
        }
        let pattern = AddressPattern::new(
            protocol(&self.protocol)?,
            &self.host,
            self.port,
            self.path.as_deref(),
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mode = self
            .path_transform
            .as_deref()
            .map(PathTransformMode::from_str)
            .transpose()
            .map_err(anyhow::Error::msg)?
            .unwrap_or_default();
        let target = AddressBuilder::default()
            .protocol(protocol(self.target_protocol.as_deref().unwrap_or("http"))?)
            .host(self.target_host.clone())
            .port(self.target_port)
            .path(self.target_path.clone())
            .path_transform_mode(mode)
            .build()?;
        Ok((pattern, target))
    }
}
