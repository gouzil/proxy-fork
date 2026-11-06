use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn custom_ca_paths_require_confirmation_before_replacing_either_file() {
    for (input, overwrite) in [
        ("", false),
        ("n\n", false),
        ("y\nn\n", false),
        ("y\ny\n", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let cert = dir.path().join("ca.pem");
        let key = dir.path().join("ca.key");
        fs_err::write(&cert, "existing certificate").unwrap();
        fs_err::write(&key, "existing private key").unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_proxy-fork-cli"))
            .args(["gen-ca", "--ca-cert"])
            .arg(&cert)
            .arg("--ca-key")
            .arg(&key)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let certificate = fs_err::read_to_string(&cert).unwrap();
        let private_key = fs_err::read_to_string(&key).unwrap();
        if overwrite {
            assert!(certificate.starts_with("-----BEGIN CERTIFICATE-----"));
            assert!(private_key.starts_with("-----BEGIN PRIVATE KEY-----"));
        } else {
            assert_eq!(certificate, "existing certificate");
            assert_eq!(private_key, "existing private key");
        }
    }
}
