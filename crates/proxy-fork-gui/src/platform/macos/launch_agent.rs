use crate::platform::{BUNDLE_ID, bundle_path, command_output};
use anyhow::{Context, Result};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

fn registration() -> Result<(String, PathBuf)> {
    let uid = command_output("/usr/bin/id", &["-u".as_ref()])?;
    let home = std::env::var_os("HOME").context("HOME is unavailable")?;
    Ok((
        format!("gui/{uid}/{BUNDLE_ID}"),
        PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{BUNDLE_ID}.plist")),
    ))
}
pub fn autostart_status() -> Result<bool> {
    let (label, path) = registration()?;
    if !path.is_file() {
        return Ok(false);
    }
    let plist = plist::Value::from_file(&path)?;
    let binary = plist
        .as_dictionary()
        .and_then(|d| d.get("ProgramArguments"))
        .and_then(plist::Value::as_array)
        .and_then(|a| a.first())
        .and_then(plist::Value::as_string)
        .context("invalid registered login item")?;
    if !Path::new(binary).is_file() {
        anyhow::bail!("registered application moved; enable autostart again");
    }
    let domain = label.rsplit_once('/').context("invalid launch domain")?.0;
    let disabled = command_output(
        "/bin/launchctl",
        &["print-disabled".as_ref(), domain.as_ref()],
    )?;
    Ok(!is_disabled(&disabled, BUNDLE_ID)?)
}
fn is_disabled(output: &str, label: &str) -> Result<bool> {
    for line in output.lines() {
        if let Some((key, value)) = line.split_once("=>")
            && key.trim().trim_matches('"') == label
        {
            return match value.trim() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => anyhow::bail!("invalid launchctl disabled state"),
            };
        }
    }
    Ok(false)
}
pub fn launch_agent_bytes(bundle: &Path) -> Result<Vec<u8>> {
    let binary = bundle.join("Contents/MacOS/proxy-fork-gui");
    anyhow::ensure!(
        bundle.extension().is_some_and(|e| e == "app") && binary.is_file(),
        "invalid installed application path"
    );
    let mut dict = plist::Dictionary::new();
    dict.insert("Label".into(), BUNDLE_ID.into());
    dict.insert(
        "ProgramArguments".into(),
        plist::Value::Array(vec![
            binary.display().to_string().into(),
            "--background".into(),
        ]),
    );
    dict.insert("RunAtLoad".into(), true.into());
    let mut bytes = Vec::new();
    plist::Value::Dictionary(dict).to_writer_xml(&mut bytes)?;
    Ok(bytes)
}
pub fn set_autostart(enabled: bool) -> Result<bool> {
    let (label, path) = registration()?;
    let bytes = if enabled {
        let bundle = bundle_path()?;
        command_output(
            "/usr/bin/codesign",
            &["--verify".as_ref(), "--strict".as_ref(), bundle.as_os_str()],
        )?;
        Some(launch_agent_bytes(&bundle)?)
    } else {
        None
    };
    configure_autostart(&path, &label, bytes.as_deref(), |args| {
        command_output("/bin/launchctl", args)
    })?;
    autostart_status()
}

fn configure_autostart(
    path: &Path,
    label: &str,
    bytes: Option<&[u8]>,
    mut launchctl: impl FnMut(&[&OsStr]) -> Result<String>,
) -> Result<()> {
    let Some(bytes) = bytes else {
        if path.exists() {
            fs_err::remove_file(path)?;
        }
        // bootout would SIGTERM the GUI when this job launched it at login.
        launchctl(&["disable".as_ref(), label.as_ref()])?;
        return Ok(());
    };
    proxy_fork_core::app_config::atomic_write(path, bytes)?;
    launchctl(&["enable".as_ref(), label.as_ref()])?;
    let domain = label.rsplit_once('/').context("invalid launch domain")?.0;
    if launchctl(&["print".as_ref(), label.as_ref()]).is_err() {
        launchctl(&["bootstrap".as_ref(), domain.as_ref(), path.as_os_str()])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loaded_but_disabled_job_is_not_reported_as_enabled() {
        assert!(is_disabled("disabled services = {\n \"test\" => true\n}", "test").unwrap());
        assert!(!is_disabled("disabled services = {\n \"test\" => false\n}", "test").unwrap());
        assert!(!is_disabled("disabled services = {}", "test").unwrap());
        assert!(is_disabled("\"test\" => unknown", "test").is_err());
    }
    #[test]
    fn autostart_changes_never_bootout_the_running_gui_job() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("agent.plist");
        for bytes in [Some(b"new plist".as_slice()), None] {
            fs_err::write(&path, "old plist").unwrap();
            let mut commands = Vec::new();
            configure_autostart(&path, "gui/501/test", bytes, |args| {
                commands.push(args[0].to_string_lossy().into_owned());
                Ok(String::new())
            })
            .unwrap();
            assert!(!commands.iter().any(|command| command == "bootout"));
            assert_eq!(path.exists(), bytes.is_some());
        }
    }
    #[test]
    fn launch_agent_keeps_spaces_as_one_argument_and_rejects_stale_path() {
        let temp = tempfile::tempdir().unwrap();
        let app = temp.path().join("Proxy Fork.app");
        assert!(launch_agent_bytes(&app).is_err());
        fs_err::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs_err::write(app.join("Contents/MacOS/proxy-fork-gui"), "test").unwrap();
        let bytes = launch_agent_bytes(&app).unwrap();
        let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).unwrap();
        let args = value.as_dictionary().unwrap()["ProgramArguments"]
            .as_array()
            .unwrap();
        assert_eq!(args.len(), 2);
        assert!(
            args[0]
                .as_string()
                .unwrap()
                .contains("Proxy Fork.app/Contents/MacOS/proxy-fork-gui")
        );
    }
}
