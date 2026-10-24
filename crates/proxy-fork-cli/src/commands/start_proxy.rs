use crate::config::AppConfig;
use anyhow::{Context, Result};
use proxy_fork_core::{dirs::user_state_dir, proxy_service::ProxyService};

pub(crate) async fn start_proxy(cfg: &AppConfig) -> Result<()> {
    let service = ProxyService::new(
        user_state_dir()
            .context("no application state directory")?
            .join("system-proxy-restore.toml"),
    );
    service.recover().await?;
    let addr = service.start(cfg.clone()).await?;
    tracing::info!("Proxy server listening on {addr}");
    tracing::info!(
        "System proxy is {}",
        if cfg.enable_sysproxy {
            "enabled"
        } else {
            "disabled"
        }
    );
    tracing::info!(
        "CA is {}",
        if cfg.enable_ca { "enabled" } else { "disabled" }
    );
    tracing::info!("Press Ctrl+C to stop the proxy service.");
    let mut state = service.subscribe();
    tokio::select! {
        signal = tokio::signal::ctrl_c() => { signal?; }
        _ = async { loop { if state.changed().await.is_err() || matches!(*state.borrow(), proxy_fork_core::proxy_service::ProxyServiceState::Failed(_)) { break; } } } => {}
    }
    service.stop().await
}
