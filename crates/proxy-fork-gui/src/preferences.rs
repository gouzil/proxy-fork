use anyhow::{Context, Result, ensure};
use proxy_fork_core::app_config::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}
impl ThemePreference {
    pub fn native_theme(self) -> Option<winit::window::Theme> {
        match self {
            Self::System => None,
            Self::Light => Some(winit::window::Theme::Light),
            Self::Dark => Some(winit::window::Theme::Dark),
        }
    }
    pub fn resolve(self, system: Option<winit::window::Theme>) -> winit::window::Theme {
        self.native_theme()
            .or(system)
            .unwrap_or(winit::window::Theme::Light)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub locale: String,
    pub theme: ThemePreference,
    pub window_size: [f32; 2],
    pub automatic_updates: bool,
    pub last_update_check: u64,
    pub system_proxy: bool,
    pub trusted_fingerprint: Option<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        let locale = sys_locale::get_locale()
            .filter(|s| {
                s.to_lowercase().starts_with("zh")
                    && !s.to_lowercase().contains("tw")
                    && !s.to_lowercase().contains("hk")
            })
            .map(|_| "zh-CN")
            .unwrap_or("en");
        Self {
            locale: locale.into(),
            theme: ThemePreference::System,
            window_size: [1000.0, 720.0],
            automatic_updates: false,
            last_update_check: 0,
            system_proxy: false,
            trusted_fingerprint: None,
        }
    }
}
impl Preferences {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let prefs: Self = toml::from_str(&fs_err::read_to_string(path)?)?;
        prefs.validate()?;
        Ok(prefs)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.locale.as_str(), "en" | "zh-CN"),
            "unsupported GUI locale"
        );
        ensure!(
            self.window_size.iter().all(|v| v.is_finite())
                && (640.0..=7680.0).contains(&self.window_size[0])
                && (480.0..=4320.0).contains(&self.window_size[1]),
            "invalid window dimensions"
        );
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        atomic_write(path, toml::to_string_pretty(self)?.as_bytes())
            .with_context(|| format!("cannot save GUI preferences at {}", path.display()))
    }
    pub fn record_ca(&mut self, fingerprint: &str, path: &Path) -> Result<()> {
        ensure!(
            self.trusted_fingerprint
                .as_deref()
                .is_none_or(|recorded| recorded == fingerprint),
            "remove the previously recorded CA before trusting a different certificate"
        );
        self.trusted_fingerprint = Some(fingerprint.into());
        self.save(path)
    }
}

pub async fn save_configuration(
    service: &proxy_fork_core::proxy_service::ProxyService,
    config: &proxy_fork_core::app_config::AppConfig,
    config_path: std::path::PathBuf,
    preferences: Preferences,
    preferences_path: std::path::PathBuf,
) -> Result<Option<String>> {
    service.save_and_replace_rules(config, config_path).await?;
    let preferences = tokio::task::spawn_blocking(move || preferences.save(&preferences_path))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|result| result);
    Ok(preferences.err().map(|error| format!("{error:#}")))
}
