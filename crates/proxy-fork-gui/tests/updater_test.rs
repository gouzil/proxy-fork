use proxy_fork_gui::updater::{parse_release, replace_bundle_with, unpack_bundle, verify_checksum};

#[test]
fn release_rejects_prerelease_wrong_arch_and_foreign_assets() {
    let make = |prerelease: bool, name: &str, url: &str| {
        serde_json::json!({"tag_name":"v0.2.0", "draft":false, "prerelease":prerelease, "body":"Changes", "assets":[{"name":name,"browser_download_url":url,"size":100},{"name":format!("{name}.sha256"),"browser_download_url":format!("{url}.sha256"),"size":100}]}).to_string()
    };
    let name = "proxy-fork-gui-0.2.0-aarch64-apple-darwin.app.tar.gz";
    let url = format!("https://github.com/gouzil/proxy-fork/releases/download/v0.2.0/{name}");
    assert!(
        parse_release(&make(false, name, &url), "0.1.0", "aarch64-apple-darwin")
            .unwrap()
            .is_some()
    );
    assert!(
        parse_release(&make(true, name, &url), "0.1.0", "aarch64-apple-darwin")
            .unwrap()
            .is_none()
    );
    assert!(
        parse_release(&make(false, name, &url), "0.2.0", "aarch64-apple-darwin")
            .unwrap()
            .is_none()
    );
    assert!(parse_release(&make(false, name, &url), "0.1.0", "x86_64-apple-darwin").is_err());
    assert!(
        parse_release(
            &make(false, name, "https://foreign.example/asset"),
            "0.1.0",
            "aarch64-apple-darwin"
        )
        .is_err()
    );
}

#[test]
fn checksum_must_match_exact_asset_and_bytes() {
    let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert!(
        verify_checksum(
            b"abc",
            &format!("{hash}  expected.tar.gz"),
            "expected.tar.gz"
        )
        .is_ok()
    );
    assert!(verify_checksum(b"wrong", hash, "expected.tar.gz").is_err());
    assert!(verify_checksum(b"abc", &format!("{hash} other.tar.gz"), "expected.tar.gz").is_err());
}

#[test]
fn failed_launch_restores_previous_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("Proxy Fork.app");
    let new = temp.path().join("new.app");
    fs_err::create_dir(&old).unwrap();
    fs_err::write(old.join("version"), "old").unwrap();
    fs_err::create_dir(&new).unwrap();
    fs_err::write(new.join("version"), "new").unwrap();
    let relaunched = std::cell::Cell::new(false);
    assert!(
        replace_bundle_with(
            &old,
            &new,
            || anyhow::bail!("new process failed"),
            || {
                assert_eq!(fs_err::read_to_string(old.join("version"))?, "old");
                relaunched.set(true);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(fs_err::read_to_string(old.join("version")).unwrap(), "old");
    assert!(relaunched.get());
}

#[test]
fn successful_replacement_keeps_a_recoverable_backup() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("Proxy Fork.app");
    let new = temp.path().join("new.app");
    fs_err::create_dir(&old).unwrap();
    fs_err::write(old.join("version"), "old").unwrap();
    fs_err::create_dir(&new).unwrap();
    fs_err::write(new.join("version"), "new").unwrap();
    replace_bundle_with(
        &old,
        &new,
        || {
            assert_eq!(fs_err::read_to_string(old.join("version"))?, "new");
            Ok(())
        },
        || panic!("successful update must not relaunch the old app"),
    )
    .unwrap();
    let backup = old.with_file_name(format!("Proxy Fork.previous-{}.app", std::process::id()));
    assert_eq!(
        fs_err::read_to_string(backup.join("version")).unwrap(),
        "old"
    );
    assert!(!new.exists());
}

#[test]
fn manual_update_survives_quitting_the_downloading_application() {
    let state = tempfile::tempdir().unwrap();
    let directory = tempfile::tempdir_in(state.path()).unwrap();
    let bundle = directory.path().join("Proxy Fork.app");
    fs_err::create_dir(&bundle).unwrap();
    fs_err::write(bundle.join("verified"), b"verified bundle").unwrap();
    let update = proxy_fork_gui::updater::VerifiedUpdate {
        directory,
        bundle,
        version: "0.2.0".into(),
    };
    let exported = proxy_fork_gui::updater::export_manual(&update, state.path()).unwrap();
    drop(update);
    assert_eq!(
        fs_err::read(exported.join("verified")).unwrap(),
        b"verified bundle"
    );
}

#[test]
#[ignore = "opt-in local package/extractor compatibility check"]
fn packaged_update_archive_is_accepted_by_the_actual_extractor() {
    let archive =
        std::env::var_os("PROXY_FORK_GUI_TEST_ARCHIVE").expect("set the local archive path");
    let extracted = tempfile::tempdir().unwrap();
    let bundle = unpack_bundle(std::path::Path::new(&archive), extracted.path()).unwrap();
    assert!(bundle.join("Contents/MacOS/proxy-fork-gui").is_file());
    assert!(bundle.join("Contents/Info.plist").is_file());
}

fn archive_entry(path: &str, kind: tar::EntryType, data: &[u8]) -> Vec<u8> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(kind);
    header.set_mode(0o755);
    header.set_size(data.len() as u64);
    // Use raw header bytes so the traversal test does not get sanitized by the builder.
    header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
    if kind.is_symlink() {
        header.set_link_name("/etc/passwd").unwrap();
    }
    header.set_cksum();
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    archive.append(&header, data).unwrap();
    archive.into_inner().unwrap().finish().unwrap()
}

#[test]
fn archive_only_accepts_regular_files_inside_the_expected_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("update.tar.gz");
    for (name, kind) in [
        ("../escape", tar::EntryType::Regular),
        ("Proxy Fork.app/../../escape", tar::EntryType::Regular),
        ("unexpected.txt", tar::EntryType::Regular),
        ("Wrong.app/Contents/binary", tar::EntryType::Regular),
        ("Proxy Fork.app/link", tar::EntryType::Symlink),
    ] {
        let destination = tempfile::tempdir_in(temp.path()).unwrap();
        fs_err::write(&path, archive_entry(name, kind, b"")).unwrap();
        assert!(
            unpack_bundle(&path, destination.path()).is_err(),
            "accepted {name}"
        );
    }
    let destination = tempfile::tempdir_in(temp.path()).unwrap();
    fs_err::write(
        &path,
        archive_entry(
            "Proxy Fork.app/Contents/MacOS/proxy-fork-gui",
            tar::EntryType::Regular,
            b"binary",
        ),
    )
    .unwrap();
    let bundle = unpack_bundle(&path, destination.path()).unwrap();
    assert_eq!(
        fs_err::read(bundle.join("Contents/MacOS/proxy-fork-gui")).unwrap(),
        b"binary"
    );
}
