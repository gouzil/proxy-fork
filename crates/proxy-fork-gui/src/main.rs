use anyhow::{Context, Result};
use proxy_fork_core::{app_config::FileConfig, dirs, proxy_service::ProxyService};
use proxy_fork_gui::{
    app::{Action, App},
    event::{AppEvent, Outcome},
    logging::{GuiLayer, LogBuffer},
    platform,
    preferences::Preferences,
    render::Graphics,
    updater,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing_subscriber::prelude::*;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::WindowId,
};

struct Desktop {
    runtime: tokio::runtime::Runtime,
    app: App,
    graphics: Option<Graphics>,
    service: Arc<ProxyService>,
    proxy: EventLoopProxy<AppEvent>,
    logs: Arc<LogBuffer>,
    visible: Arc<AtomicBool>,
    config_path: PathBuf,
    preferences_path: PathBuf,
    state_dir: PathBuf,
    release: Option<updater::Release>,
    downloaded: Option<Arc<updater::VerifiedUpdate>>,
    deadline: Option<Instant>,
    held_keys: std::collections::HashSet<winit::keyboard::PhysicalKey>,
    background: bool,
    ready_path: Option<PathBuf>,
    #[cfg(target_os = "macos")]
    tray: Option<platform::Tray>,
}

impl Desktop {
    fn request_redraw(&self) {
        if self.visible.load(Ordering::Relaxed)
            && let Some(graphics) = &self.graphics
        {
            graphics.window.request_redraw();
        }
    }
    fn show(&mut self) {
        self.visible.store(true, Ordering::Relaxed);
        if let Some(graphics) = &self.graphics {
            graphics.window.set_visible(true);
            graphics.window.focus_window();
            graphics.window.request_redraw();
        }
    }
    fn action(&mut self, mut action: Action) {
        if action == Action::Start && self.app.running() {
            action = Action::Stop;
        }
        let Some(action) = self.app.begin(action) else {
            return;
        };
        let config = if matches!(action, Action::Start | Action::Save) {
            match self.app.edited_config() {
                Ok(config) => Some(config),
                Err(error) => {
                    self.app.finish(Err(error.to_string()));
                    self.show();
                    return;
                }
            }
        } else {
            None
        };
        let service = self.service.clone();
        let proxy = self.proxy.clone();
        let preferences_path = self.preferences_path.clone();
        let config_path = self.config_path.clone();
        let state_dir = self.state_dir.clone();
        let mut preferences = self.app.preferences.clone();
        preferences.system_proxy = self.app.config.enable_sysproxy;
        let default_cert = dirs::default_cert_path();
        let cert = if self.app.cert_path.is_empty() {
            default_cert.clone()
        } else {
            Some(PathBuf::from(&self.app.cert_path))
        };
        let is_default_cert = cert == default_cert;
        let key = if self.app.key_path.is_empty() {
            dirs::default_private_key_path()
        } else {
            Some(PathBuf::from(&self.app.key_path))
        };
        let fingerprint = self.app.ca_fingerprint.clone();
        let target_keychain = self.app.ca_keychain.clone();
        let recorded = self.app.preferences.trusted_fingerprint.clone();
        let confirmed = self.app.ca_confirm;
        let overwrite = self.app.ca_overwrite;
        let release = self.release.clone();
        let downloaded = self.downloaded.clone();
        self.runtime.spawn(async move {
            let result: Result<Outcome> = async {
                match action {
                    Action::Start => {
                        service
                            .start(config.context("missing start configuration")?)
                            .await?;
                        Ok(Outcome::Done("state.done"))
                    }
                    Action::Stop => {
                        service.stop().await?;
                        Ok(Outcome::Done("state.done"))
                    }
                    Action::Save => {
                        let config = config.context("missing configuration")?;
                        let warning = proxy_fork_gui::preferences::save_configuration(
                            &service,
                            &config,
                            config_path,
                            preferences,
                            preferences_path,
                        )
                        .await?;
                        Ok(Outcome::Saved(warning))
                    }
                    Action::SavePreferences => {
                        tokio::task::spawn_blocking(move || preferences.save(&preferences_path))
                            .await??;
                        Ok(Outcome::Preferences)
                    }
                    Action::GenerateCa => {
                        anyhow::ensure!(
                            !overwrite || recorded.is_none(),
                            "remove the recorded CA trust before overwriting its files"
                        );
                        let cert = cert.context("no certificate directory")?;
                        let key = key.context("no private key directory")?;
                        let (generated, keychain) = tokio::task::spawn_blocking(move || {
                            let generated = proxy_fork_core::generate_ca(&cert, &key, overwrite)?;
                            Ok::<_, anyhow::Error>((generated, platform::keychain_path().ok()))
                        })
                        .await??;
                        Ok(Outcome::Certificate(generated, keychain))
                    }
                    Action::InspectCa => {
                        let cert = cert.context("no certificate directory")?;
                        tokio::task::spawn_blocking(move || {
                            let recorded =
                                Preferences::load(&preferences_path)?.trusted_fingerprint;
                            let Some((fingerprint, trusted)) =
                                platform::inspect_configured_ca(&cert, is_default_cert)?
                            else {
                                return Ok(Outcome::CaNotConfigured(recorded));
                            };
                            Ok::<_, anyhow::Error>(Outcome::Trust {
                                fingerprint,
                                trusted,
                                recorded,
                                keychain: platform::keychain_path().ok(),
                            })
                        })
                        .await?
                    }
                    Action::TrustCa => {
                        anyhow::ensure!(
                            confirmed,
                            "certificate trust requires explicit confirmation"
                        );
                        let cert = cert.context("no certificate directory")?;
                        let fingerprint =
                            fingerprint.context("inspect the certificate before trusting it")?;
                        let keychain = target_keychain
                            .context("inspect the target Keychain before trusting a certificate")?;
                        let confirmed_keychain = keychain.clone();
                        let recorded = Some(fingerprint.clone());
                        let intent = fingerprint.clone();
                        // Persist ownership before modifying Keychain, including partially failed installs.
                        tokio::task::spawn_blocking(move || {
                            preferences.record_ca(&intent, &preferences_path)
                        })
                        .await??;
                        let (fingerprint, trusted) = tokio::task::spawn_blocking(move || {
                            platform::trust_ca(&cert, &fingerprint, &confirmed_keychain)
                        })
                        .await??;
                        Ok(Outcome::Trust {
                            fingerprint,
                            trusted,
                            recorded,
                            keychain: Some(keychain),
                        })
                    }
                    Action::RemoveCa => {
                        anyhow::ensure!(
                            confirmed,
                            "certificate removal requires explicit confirmation"
                        );
                        let cert = cert.context("no certificate directory")?;
                        let recorded =
                            recorded.context("no application certificate is recorded")?;
                        let fingerprint = recorded.clone();
                        tokio::task::spawn_blocking(move || platform::remove_ca(&cert, &recorded))
                            .await??;
                        preferences.trusted_fingerprint = None;
                        tokio::task::spawn_blocking(move || preferences.save(&preferences_path))
                            .await??;
                        Ok(Outcome::Trust {
                            fingerprint,
                            trusted: false,
                            recorded: None,
                            keychain: target_keychain,
                        })
                    }
                    Action::EnableAutostart | Action::DisableAutostart => {
                        let enabled = action == Action::EnableAutostart;
                        Ok(Outcome::Autostart(
                            tokio::task::spawn_blocking(move || platform::set_autostart(enabled))
                                .await??,
                        ))
                    }
                    Action::InspectAutostart => Ok(Outcome::Autostart(
                        tokio::task::spawn_blocking(platform::autostart_status).await??,
                    )),
                    Action::CheckUpdate => {
                        preferences.last_update_check =
                            SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                        tokio::task::spawn_blocking(move || preferences.save(&preferences_path))
                            .await??;
                        Ok(Outcome::Update(updater::check().await?))
                    }
                    Action::DownloadUpdate => {
                        let release = release.context("no update is available")?;
                        let progress = proxy.clone();
                        let update =
                            updater::download(release, state_dir, move |received, total| {
                                let _ = progress
                                    .send_event(AppEvent::DownloadProgress(received, total));
                            })
                            .await?;
                        Ok(Outcome::Downloaded(update))
                    }
                    Action::InstallUpdate => {
                        let update = downloaded.context("no verified update is available")?;
                        let target = tokio::task::spawn_blocking(platform::bundle_path).await??;
                        let writable = tokio::task::spawn_blocking(move || {
                            tempfile::tempfile_in(
                                target.parent().context("invalid installation directory")?,
                            )
                            .map(|_| ())
                            .map_err(anyhow::Error::from)
                        })
                        .await?;
                        if writable.is_err() {
                            let bundle = tokio::task::spawn_blocking(move || {
                                let bundle = updater::export_manual(&update, &state_dir)?;
                                platform::command_output(
                                    "/usr/bin/open",
                                    &["-R".as_ref(), bundle.as_os_str()],
                                )?;
                                Ok::<_, anyhow::Error>(bundle)
                            })
                            .await??;
                            return Ok(Outcome::ManualUpdate(bundle));
                        }
                        service.stop().await?;
                        tokio::task::spawn_blocking(move || preferences.save(&preferences_path))
                            .await??;
                        let target =
                            tokio::task::spawn_blocking(move || updater::prepare_install(&update))
                                .await??;
                        anyhow::ensure!(
                            target.is_some(),
                            "installation directory became unwritable"
                        );
                        Ok(Outcome::Exit)
                    }
                    Action::Exit => {
                        service.stop().await?;
                        tokio::task::spawn_blocking(move || preferences.save(&preferences_path))
                            .await??;
                        Ok(Outcome::Exit)
                    }
                }
            }
            .await;
            let _ = proxy.send_event(AppEvent::Finished(result.map_err(|e| format!("{e:#}"))));
        });
        self.request_redraw();
    }
    fn finished(
        &mut self,
        result: std::result::Result<Outcome, String>,
        event_loop: &ActiveEventLoop,
    ) {
        match result {
            Err(error) => {
                tracing::warn!("Desktop operation failed");
                self.app.finish(Err(error));
                self.show();
            }
            Ok(outcome) => {
                let mut message = "state.done";
                let mut notice = None;
                match outcome {
                    Outcome::Done(key) => message = key,
                    Outcome::Saved(warning) => {
                        message = "state.saved";
                        self.app.preferences.system_proxy = self.app.config.enable_sysproxy;
                        if let Some(warning) = warning {
                            self.app.error = Some(format!(
                                "{}: {warning}",
                                proxy_fork_gui::_rust_i18n_translate(
                                    &rust_i18n::locale(),
                                    "state.preferences_failed"
                                )
                            ));
                            self.app.exiting = false;
                            self.show();
                        }
                    }
                    Outcome::Preferences => {
                        message = "state.saved";
                    }
                    Outcome::Certificate(ca, keychain) => {
                        self.app.cert_path = ca.cert_path.display().to_string();
                        self.app.key_path = ca.key_path.display().to_string();
                        self.app.ca_fingerprint = Some(ca.fingerprint);
                        self.app.ca_keychain = keychain;
                        self.app.ca_trusted = Some(false);
                        self.app.ca_missing = false;
                        self.app.ca_confirm = false;
                        self.app.ca_overwrite = false;
                    }
                    Outcome::CaNotConfigured(recorded) => {
                        self.app.ca_not_configured(recorded);
                        message = "ca.not_configured";
                    }
                    Outcome::Trust {
                        fingerprint,
                        trusted,
                        recorded,
                        keychain,
                    } => {
                        self.app.ca_fingerprint = Some(fingerprint.clone());
                        self.app.ca_keychain = keychain;
                        self.app.ca_trusted = Some(trusted);
                        self.app.ca_missing = false;
                        self.app.preferences.trusted_fingerprint = recorded;
                        self.app.ca_confirm = false;
                    }
                    Outcome::Autostart(enabled) => self.app.autostart = Some(enabled),
                    Outcome::Update(release) => {
                        self.app.preferences.last_update_check = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        self.app.update_version = release.as_ref().map(|r| r.version.clone());
                        self.app.update_notes = release
                            .as_ref()
                            .map(|r| r.notes.clone())
                            .unwrap_or_default();
                        if release.is_some() {
                            self.app.update_dialog = true;
                            self.show();
                        } else {
                            message = "update.current";
                        }
                        self.release = release;
                        self.downloaded = None;
                        self.app.update_downloaded = false;
                    }
                    Outcome::Downloaded(update) => {
                        self.downloaded = Some(Arc::new(update));
                        self.app.update_downloaded = true;
                        message = "update.ready";
                    }
                    Outcome::Exit => {
                        self.app.exit_ready = true;
                        event_loop.exit();
                    }
                    Outcome::ManualUpdate(path) => {
                        notice = Some(format!(
                            "{}\n{}",
                            proxy_fork_gui::_rust_i18n_translate(
                                &rust_i18n::locale(),
                                "update.manual"
                            ),
                            path.display()
                        ));
                    }
                }
                self.app
                    .finish(Ok(notice.unwrap_or_else(|| message.into())));
            }
        }
        if let Some(action) = self.app.queued_action() {
            self.action(action);
        }
        self.request_redraw();
    }
}
impl ApplicationHandler<AppEvent> for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        match self.runtime.block_on(Graphics::new(
            event_loop,
            self.app.preferences.window_size,
            self.app.preferences.theme,
        )) {
            Ok(graphics) => {
                tracing::info!("GUI initialized; font atlas {} bytes", graphics.font_bytes);
                self.graphics = Some(graphics);
                #[cfg(target_os = "macos")]
                {
                    match platform::Tray::new(self.proxy.clone()) {
                        Ok(tray) => self.tray = Some(tray),
                        Err(error) => {
                            self.app.error = Some(format!("Tray initialization failed: {error}"));
                            self.background = false;
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    self.background = false;
                }
                if self.background
                    && let Some(g) = &self.graphics
                {
                    g.window.set_visible(false);
                    self.visible.store(false, Ordering::Relaxed);
                }
                self.request_redraw();
                if let Some(path) = self.ready_path.take()
                    && let Err(error) = proxy_fork_core::app_config::atomic_write(&path, b"ready")
                {
                    self.app.error = Some(error.to_string());
                }
            }
            Err(error) => {
                tracing::error!("GUI initialization failed: {error:#}");
                self.app.error = Some(error.to_string());
                event_loop.exit();
            }
        }
    }
    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.request_redraw();
        }
    }
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .graphics
            .as_ref()
            .is_none_or(|g| g.window.id() != window_id)
        {
            return;
        }
        if let Some(graphics) = &mut self.graphics {
            graphics.event(event.clone());
        }
        match event {
            WindowEvent::CloseRequested => {
                #[cfg(target_os = "macos")]
                let has_tray = self.tray.is_some();
                #[cfg(not(target_os = "macos"))]
                let has_tray = false;
                if has_tray {
                    self.visible.store(false, Ordering::Relaxed);
                    self.deadline = None;
                    self.held_keys.clear();
                    if let Some(g) = &self.graphics {
                        g.window.set_visible(false);
                    }
                } else {
                    self.action(Action::Exit);
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.visible.store(
                    !occluded
                        && self
                            .graphics
                            .as_ref()
                            .is_some_and(|g| g.window.is_visible().unwrap_or(true)),
                    Ordering::Relaxed,
                );
                self.request_redraw();
            }
            WindowEvent::Resized(_) => {
                if let Some(g) = &mut self.graphics {
                    g.resize();
                    let logical = g
                        .window
                        .inner_size()
                        .to_logical::<f32>(g.window.scale_factor());
                    self.app.preferences.window_size = [
                        logical.width.clamp(640.0, 7680.0),
                        logical.height.clamp(480.0, 4320.0),
                    ];
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(g) = &mut self.graphics {
                    g.dpi_changed();
                }
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    self.held_keys.insert(event.physical_key);
                } else {
                    self.held_keys.remove(&event.physical_key);
                }
                self.request_redraw();
            }
            WindowEvent::Focused(false) => {
                self.held_keys.clear();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if self.visible.load(Ordering::Relaxed) {
                    let locale = self.app.preferences.locale.clone();
                    let theme = self.app.preferences.theme;
                    let result = self
                        .graphics
                        .as_mut()
                        .map(|g| g.draw(&mut self.app, &self.logs));
                    match result {
                        Some(Ok(actions)) => {
                            for action in actions {
                                self.action(action);
                            }
                        }
                        Some(Err(error)) => {
                            self.app.error = Some(format!("Rendering failed: {error:#}"));
                        }
                        None => {}
                    }
                    if theme != self.app.preferences.theme
                        && let Some(graphics) = &mut self.graphics
                    {
                        graphics.set_theme(self.app.preferences.theme);
                    }
                    if locale != self.app.preferences.locale || theme != self.app.preferences.theme
                    {
                        self.app.preferences_dirty = true;
                        if let Some(action) = self.app.queued_action() {
                            self.action(action);
                        }
                        self.request_redraw();
                    }
                }
            }
            _ => self.request_redraw(),
        }
        #[cfg(target_os = "macos")]
        if let Some(tray) = &self.tray {
            tray.update(self.app.running(), self.app.busy);
        }
        let _ = event_loop;
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Action(action) => self.action(action),
            AppEvent::Finished(result) => self.finished(result, event_loop),
            AppEvent::State(state) => {
                if matches!(
                    state,
                    proxy_fork_core::proxy_service::ProxyServiceState::Running(_)
                ) {
                    self.app.restart_required = false;
                }
                self.app.state = state;
                self.request_redraw();
            }
            AppEvent::Show => self.show(),
            AppEvent::LogsReady => self.request_redraw(),
            AppEvent::DownloadProgress(done, total) => {
                self.app.update_progress = (done, total);
                self.request_redraw();
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(tray) = &self.tray {
            tray.update(self.app.running(), self.app.busy);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let active = self.visible.load(Ordering::Relaxed)
            && self
                .graphics
                .as_ref()
                .is_some_and(|g| g.window.has_focus() && g.text_active);
        if active || (self.visible.load(Ordering::Relaxed) && !self.held_keys.is_empty()) {
            let deadline = Instant::now()
                + if self.held_keys.is_empty() {
                    Duration::from_millis(250)
                } else {
                    Duration::from_millis(16)
                };
            self.deadline = Some(deadline);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            self.deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--internal-update") {
        anyhow::ensure!(args.len() == 5, "invalid internal updater arguments");
        return updater::internal_update(
            args[1]
                .to_str()
                .context("invalid old process id")?
                .parse()?,
            &PathBuf::from(&args[2]),
            &PathBuf::from(&args[3]),
            args[4].to_str().context("invalid version")?,
        );
    }
    let background = args.iter().any(|a| a == "--background");
    let ready_path = if args.first().is_some_and(|a| a == "--update-ready") {
        anyhow::ensure!(args.len() == 2, "invalid readiness arguments");
        let path = PathBuf::from(&args[1]);
        anyhow::ensure!(
            path.file_name().is_some_and(|n| n == "ready")
                && path
                    .parent()
                    .and_then(std::path::Path::file_name)
                    .is_some_and(|s| s.to_string_lossy().starts_with(".proxy-fork-update-")),
            "invalid update readiness path"
        );
        Some(path)
    } else {
        None
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let config_dir =
        dirs::user_proxy_fork_config_dir().context("no user configuration directory")?;
    let state_dir = dirs::user_state_dir().context("no user state directory")?;
    fs_err::create_dir_all(&state_dir)?;
    let lock = fs_err::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state_dir.join("gui.lock"))?;
    lock.file()
        .try_lock()
        .context("another Proxy Fork desktop instance is running")?;
    let config_path = config_dir.join("config.toml");
    let preferences_path = config_dir.join("gui.toml");
    let config = if config_path.exists() {
        FileConfig::load(&config_path).and_then(|f| f.runtime())
    } else {
        Ok(Default::default())
    };
    let preferences = Preferences::load(&preferences_path);
    let startup_error = config
        .as_ref()
        .err()
        .or_else(|| preferences.as_ref().err())
        .map(|e| e.to_string());
    let mut app = App::new(config.unwrap_or_default(), preferences.unwrap_or_default());
    rust_i18n::set_locale(&app.preferences.locale);
    app.error = startup_error;
    if args.iter().any(|arg| arg == "--update-rolled-back") {
        app.error = Some(
            proxy_fork_gui::_rust_i18n_translate(&rust_i18n::locale(), "update.rolled_back")
                .into_owned(),
        );
    }
    app.busy = true;
    let logs = Arc::new(LogBuffer::default());
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "proxy_fork_gui=info,proxy_fork_core=info,hudsucker=warn",
                )
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(GuiLayer(logs.clone()))
        .init();
    proxy_fork_gui::translation_diagnostics();
    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let service = Arc::new(ProxyService::new(
        state_dir.join("system-proxy-restore.toml"),
    ));
    let mut states = service.subscribe();
    let state_proxy = proxy.clone();
    runtime.spawn(async move {
        loop {
            if states.changed().await.is_err() {
                break;
            }
            let state = states.borrow_and_update().clone();
            if state_proxy.send_event(AppEvent::State(state)).is_err() {
                break;
            }
        }
    });
    let visible = Arc::new(AtomicBool::new(true));
    let log_buffer = logs.clone();
    let log_proxy = proxy.clone();
    let log_visible = visible.clone();
    runtime.spawn(async move {
        loop {
            log_buffer.changed.notified().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            if log_visible.load(Ordering::Relaxed)
                && log_proxy.send_event(AppEvent::LogsReady).is_err()
            {
                break;
            }
        }
    });
    let recovery = service.clone();
    let startup_proxy = proxy.clone();
    runtime.spawn(async move {
        let result = recovery
            .recover()
            .await
            .map(|_| Outcome::Done("state.done"))
            .map_err(|e| format!("{e:#}"));
        let _ = startup_proxy.send_event(AppEvent::Finished(result));
    });
    let signal_proxy = proxy.clone();
    runtime.spawn(async move {
        #[cfg(unix)]
        {
            let mut terminate =
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                    Ok(signal) => signal,
                    Err(_) => return,
                };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
        }
        #[cfg(not(unix))]
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        let _ = signal_proxy.send_event(AppEvent::Action(Action::Exit));
    });
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if app.preferences.automatic_updates
        && now.saturating_sub(app.preferences.last_update_check) >= 86400
        && cfg!(target_os = "macos")
    {
        let update_proxy = proxy.clone();
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let _ = update_proxy.send_event(AppEvent::Action(Action::CheckUpdate));
        });
    }
    let mut desktop = Desktop {
        runtime,
        app,
        graphics: None,
        service,
        proxy,
        logs,
        visible,
        config_path,
        preferences_path,
        state_dir,
        release: None,
        downloaded: None,
        deadline: None,
        held_keys: Default::default(),
        background,
        ready_path,
        #[cfg(target_os = "macos")]
        tray: None,
    };
    event_loop.run_app(&mut desktop)?;
    if !desktop.app.exit_ready {
        desktop.runtime.block_on(desktop.service.stop())?;
    }
    if let Some(graphics) = desktop.graphics {
        tracing::info!(
            "GUI stopped after {} frames; font atlas {} bytes",
            graphics.frames,
            graphics.font_bytes
        );
    }
    drop(lock);
    if let Some(error) = desktop.app.error {
        anyhow::bail!(error);
    }
    Ok(())
}
