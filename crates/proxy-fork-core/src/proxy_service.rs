use crate::{
    CaEnum, CertInput, NoCa, Proxy, ProxyHandlerBuilder, ProxyManager,
    app_config::{AppConfig, atomic_write},
    dirs, load_ca_from_sources,
    rustls::crypto::aws_lc_rs,
    system_proxy::ProxySettings,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{
    sync::{Mutex, RwLock, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

pub type ProxyServiceError = anyhow::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyServiceState {
    Stopped,
    Starting,
    Running(SocketAddr),
    Stopping,
    Failed(String),
}

struct Running {
    cancel: CancellationToken,
    task: JoinHandle<Result<()>>,
    manager: Arc<RwLock<ProxyManager>>,
}

pub struct ProxyService {
    running: Mutex<Option<Running>>,
    state: watch::Sender<ProxyServiceState>,
    restore_path: PathBuf,
}

impl ProxyService {
    pub fn new(restore_path: PathBuf) -> Self {
        let (state, _) = watch::channel(ProxyServiceState::Stopped);
        Self {
            running: Mutex::new(None),
            state,
            restore_path,
        }
    }
    pub fn subscribe(&self) -> watch::Receiver<ProxyServiceState> {
        self.state.subscribe()
    }

    pub async fn start(&self, cfg: AppConfig) -> Result<SocketAddr> {
        let mut running = self.running.lock().await;
        if running.as_ref().is_some_and(|r| !r.task.is_finished()) {
            bail!("proxy is already running");
        }
        if let Some(previous) = running.take()
            && let Err(error) = previous
                .task
                .await
                .context("proxy task failed")
                .and_then(|r| r)
        {
            tracing::warn!(%error, "Previous proxy task failed; retrying startup");
        }
        self.state.send_replace(ProxyServiceState::Starting);
        let result = self.start_inner(cfg).await;
        match result {
            Ok((addr, r)) => {
                *running = Some(r);
                self.state.send_replace(ProxyServiceState::Running(addr));
                Ok(addr)
            }
            Err(error) => {
                self.state
                    .send_replace(ProxyServiceState::Failed(error.to_string()));
                Err(error)
            }
        }
    }

    async fn start_inner(&self, cfg: AppConfig) -> Result<(SocketAddr, Running)> {
        let manager = Arc::new(RwLock::new(cfg.build_manager().await?));
        let ca_cfg = cfg.clone();
        let ca = tokio::task::spawn_blocking(move || build_ca(&ca_cfg)).await??;
        let host = cfg.listen.host.trim_matches(['[', ']']);
        let addresses = tokio::net::lookup_host((host, cfg.listen.port)).await?;
        let mut addresses: Vec<_> = addresses.collect();
        addresses.sort_by_key(|a| a.is_ipv6());
        let address = addresses
            .first()
            .context("listen host resolved to no addresses")?;
        let listener = tokio::net::TcpListener::bind(address).await?;
        let addr = listener.local_addr()?;
        let handler = ProxyHandlerBuilder::default()
            .proxy_manager(manager.clone())
            .with_ca(cfg.enable_ca)
            .build()?;
        let cancel = CancellationToken::new();
        let shutdown = cancel.clone();
        let proxy = Proxy::builder()
            .with_listener(listener)
            .with_ca(ca)
            .with_rustls_connector(aws_lc_rs::default_provider())
            .with_http_handler(handler.clone())
            .with_websocket_handler(handler)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .build()?;
        let mut proxy_lock = None;
        if cfg.enable_sysproxy {
            let path = self.restore_path.clone();
            proxy_lock = Some(
                tokio::task::spawn_blocking(move || {
                    let lock = lock_recovery(&path)?;
                    if path.exists() {
                        bail!(
                            "unresolved system proxy recovery record: {}",
                            path.display()
                        );
                    }
                    let original = ProxySettings::read()?;
                    let written = original.for_listener(addr)?;
                    let record = RestoreRecord { original, written };
                    atomic_write(&path, toml::to_string(&record)?.as_bytes())?;
                    // The recovery journal must be durable before any system setting is changed.
                    #[cfg(unix)]
                    fs_err::File::open(path.parent().context("no recovery directory")?)?
                        .sync_all()?;
                    let write = record.written.write(&record.original).and_then(|()| {
                        anyhow::ensure!(
                            record.written.read_current()? == record.written,
                            "system proxy write could not be verified"
                        );
                        Ok(())
                    });
                    if let Err(error) = write {
                        // A setter may partially succeed; keep the record until restoration succeeds.
                        let _ =
                            restore_with(&path, ProxySettings::read_current, ProxySettings::write);
                        return Err(error);
                    }
                    Ok(lock)
                })
                .await??,
            );
        }
        let path = self.restore_path.clone();
        let restore = cfg.enable_sysproxy;
        let state = self.state.clone();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            let _proxy_lock = proxy_lock;
            let result = proxy
                .start()
                .await
                .map_err(anyhow::Error::from)
                .and_then(|()| {
                    anyhow::ensure!(task_cancel.is_cancelled(), "proxy stopped unexpectedly");
                    Ok(())
                });
            let result = if restore {
                let cleanup = tokio::task::spawn_blocking(move || {
                    restore_with(&path, ProxySettings::read_current, ProxySettings::write)
                })
                .await
                .context("system proxy cleanup task failed")
                .and_then(|r| r);
                result.and(cleanup)
            } else {
                result
            };
            if let Err(error) = &result {
                state.send_replace(ProxyServiceState::Failed(error.to_string()));
            }
            result
        });
        tracing::info!(%addr, "Proxy service startup complete. Ready to accept requests.");
        Ok((
            addr,
            Running {
                cancel,
                task,
                manager,
            },
        ))
    }

    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.lock().await;
        if let Some(r) = running.take() {
            self.state.send_replace(ProxyServiceState::Stopping);
            r.cancel.cancel();
            if let Err(error) = r.task.await.context("proxy task failed").and_then(|r| r) {
                self.state
                    .send_replace(ProxyServiceState::Failed(error.to_string()));
                return Err(error);
            }
        }
        self.recover().await?;
        self.state.send_replace(ProxyServiceState::Stopped);
        Ok(())
    }

    pub async fn replace_rules(&self, cfg: &AppConfig) -> Result<()> {
        let manager = cfg.build_manager().await?;
        let running = self.running.lock().await;
        if let Some(r) = running.as_ref() {
            *r.manager.write().await = manager;
        }
        Ok(())
    }

    pub async fn save_and_replace_rules(&self, cfg: &AppConfig, path: PathBuf) -> Result<()> {
        let manager = cfg.build_manager().await?;
        let running = self.running.lock().await;
        let disk = cfg.file_config();
        tokio::task::spawn_blocking(move || disk.save(&path)).await??;
        if let Some(r) = running.as_ref() {
            *r.manager.write().await = manager;
        }
        Ok(())
    }

    pub async fn recover(&self) -> Result<()> {
        let path = self.restore_path.clone();
        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return Ok(());
            }
            let _lock = lock_recovery(&path)?;
            restore_with(&path, ProxySettings::read_current, ProxySettings::write)
        })
        .await?
    }
}

fn lock_recovery(path: &std::path::Path) -> Result<fs_err::File> {
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    let file = fs_err::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    file.file()
        .try_lock()
        .context("another proxy process owns the system proxy recovery record")?;
    Ok(file)
}

fn build_ca(cfg: &AppConfig) -> Result<CaEnum> {
    if !cfg.enable_ca {
        return Ok(CaEnum::None(NoCa));
    }
    let cert = cfg
        .ca_cert
        .clone()
        .or_else(dirs::default_cert_path)
        .context("no CA certificate directory")?;
    let key = cfg
        .ca_key
        .clone()
        .or_else(dirs::default_private_key_path)
        .context("no CA key directory")?;
    let source = if cfg.ca_cert.is_none() && cfg.ca_key.is_some() {
        CertInput::System(dirs::APP_NAME)
    } else {
        CertInput::File(cert.to_str().context("invalid certificate path")?)
    };
    load_ca_from_sources(
        source,
        CertInput::File(key.to_str().context("invalid key path")?),
    )
    .map(CaEnum::Openssl)
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[derive(Serialize, Deserialize)]
struct RestoreRecord {
    original: ProxySettings,
    written: ProxySettings,
}
fn restore_with(
    path: &std::path::Path,
    mut read: impl FnMut(&ProxySettings) -> Result<ProxySettings>,
    write: impl FnOnce(&ProxySettings, &ProxySettings) -> Result<()>,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let record: RestoreRecord = toml::from_str(&fs_err::read_to_string(path)?)?;
    let current = read(&record.written)?;
    if current != record.written {
        if current == record.original {
            fs_err::remove_file(path)?;
            return Ok(());
        }
        bail!(
            "system proxy was changed externally; recovery record retained at {}",
            path.display()
        );
    }
    write(&record.original, &record.written)?;
    anyhow::ensure!(
        read(&record.original)? == record.original,
        "system proxy restoration could not be verified; recovery record retained at {}",
        path.display()
    );
    fs_err::remove_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_verifies_restored_settings_before_removing_its_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restore.toml");
        let original = ProxySettings::default();
        let written = original
            .for_listener("127.0.0.1:7898".parse().unwrap())
            .unwrap();
        atomic_write(
            &path,
            toml::to_string(&RestoreRecord {
                original,
                written: written.clone(),
            })
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
        assert!(restore_with(&path, |_| Ok(written.clone()), |_, _| Ok(())).is_err());
        assert!(path.exists());
    }

    #[test]
    fn recovery_preserves_external_changes_and_keeps_failed_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restore.toml");
        let original = ProxySettings::default();
        let written = original
            .for_listener("127.0.0.1:7898".parse().unwrap())
            .unwrap();
        let record = RestoreRecord {
            original: original.clone(),
            written: written.clone(),
        };
        atomic_write(&path, toml::to_string(&record).unwrap().as_bytes()).unwrap();
        let external = written
            .for_listener("127.0.0.1:1234".parse().unwrap())
            .unwrap();
        assert!(
            restore_with(
                &path,
                |_| Ok(external.clone()),
                |_, _| panic!("must not overwrite external settings")
            )
            .is_err()
        );
        assert!(path.exists());
        assert!(
            restore_with(
                &path,
                |_| Ok(written.clone()),
                |_, _| bail!("permission denied")
            )
            .is_err()
        );
        assert!(path.exists());
        let mut reads = [written.clone(), original.clone()].into_iter();
        restore_with(
            &path,
            |_| Ok(reads.next().unwrap()),
            |p, expected| {
                assert_eq!(p, &original);
                assert_eq!(expected, &written);
                Ok(())
            },
        )
        .unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn recovery_cannot_steal_an_active_process_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restore.toml");
        let owner = lock_recovery(&path).unwrap();
        assert!(lock_recovery(&path).is_err());
        drop(owner);
        assert!(lock_recovery(&path).is_ok());
    }
}
