use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub fn replace_bundle_with(
    target: &Path,
    new: &Path,
    launch: impl FnOnce() -> Result<()>,
    relaunch_previous: impl FnOnce() -> Result<()>,
) -> Result<()> {
    ensure!(
        target.extension().is_some_and(|e| e == "app")
            && new.extension().is_some_and(|e| e == "app"),
        "invalid application bundle paths"
    );
    let backup = target.with_file_name(format!(
        "{}.previous-{}.app",
        target.file_stem().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    ensure!(
        !backup.exists(),
        "a previous update backup requires manual recovery"
    );
    fs_err::rename(target, &backup)?;
    if let Err(error) = fs_err::rename(new, target) {
        fs_err::rename(&backup, target).context("failed to restore application backup")?;
        relaunch_previous().context("previous application restored but failed to relaunch")?;
        return Err(error.into());
    }
    if let Err(error) = launch() {
        fs_err::rename(target, new).context("failed to remove unsuccessful update")?;
        fs_err::rename(&backup, target).context("failed to restore application backup")?;
        relaunch_previous().context("previous application restored but failed to relaunch")?;
        return Err(error);
    }
    Ok(())
}
fn copy_bundle(source: &Path, destination: &Path) -> Result<()> {
    fs_err::create_dir(destination)?;
    for entry in fs_err::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_bundle(&entry.path(), &target)?;
        } else {
            ensure!(kind.is_file(), "application contains unsupported links");
            fs_err::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn export_manual(update: &super::VerifiedUpdate, state_dir: &Path) -> Result<PathBuf> {
    let exported = tempfile::Builder::new()
        .prefix("manual-update-")
        .tempdir_in(state_dir)?;
    let bundle = exported.path().join("Proxy Fork.app");
    copy_bundle(&update.bundle, &bundle)?;
    let _ = exported.keep();
    Ok(bundle)
}

pub fn prepare_install(update: &super::VerifiedUpdate) -> Result<Option<PathBuf>> {
    let target = crate::platform::bundle_path()?;
    let parent = target.parent().context("no application parent directory")?;
    let stage = match tempfile::Builder::new()
        .prefix(".proxy-fork-update-")
        .tempdir_in(parent)
    {
        Ok(stage) => stage,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let staged = stage.path().join("Proxy Fork.app");
    copy_bundle(&update.bundle, &staged)?;
    super::verify_bundle(&staged, &update.version, &target)?;
    let helper = staged.join("Contents/MacOS/proxy-fork-gui");
    std::process::Command::new(helper)
        .arg("--internal-update")
        .arg(std::process::id().to_string())
        .arg(&target)
        .arg(&staged)
        .arg(&update.version)
        .spawn()?;
    let _ = stage.keep();
    Ok(Some(target))
}

pub fn internal_update(pid: u32, target: &Path, staged: &Path, version: &str) -> Result<()> {
    ensure!(
        staged.parent().is_some_and(|p| p
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with(".proxy-fork-update-")))
            && target.parent() == staged.parent().and_then(Path::parent),
        "invalid staged update location"
    );
    super::verify_bundle(staged, version, target)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while process_alive(pid)? {
        ensure!(Instant::now() < deadline, "old application did not exit");
        std::thread::sleep(Duration::from_millis(200));
    }
    let ready = staged
        .parent()
        .context("missing stage directory")?
        .join("ready");
    replace_bundle_with(
        target,
        staged,
        || {
            let mut child =
                std::process::Command::new(target.join("Contents/MacOS/proxy-fork-gui"))
                    .arg("--update-ready")
                    .arg(&ready)
                    .spawn()?;
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                if ready.exists() {
                    return Ok(());
                }
                if let Some(status) = child.try_wait()? {
                    anyhow::bail!("updated application exited before becoming ready: {status}");
                }
                if Instant::now() >= deadline {
                    child.kill()?;
                    child.wait()?;
                    anyhow::bail!("updated application did not become ready");
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        },
        || {
            std::process::Command::new(target.join("Contents/MacOS/proxy-fork-gui"))
                .arg("--update-rolled-back")
                .spawn()?;
            Ok(())
        },
    )?;
    Ok(())
}
fn process_alive(pid: u32) -> Result<bool> {
    #[cfg(unix)]
    {
        Ok(std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .output()?
            .status
            .success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        anyhow::bail!("update installation is unsupported")
    }
}
