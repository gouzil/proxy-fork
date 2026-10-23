#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_listener_without_touching_system_proxy() {
    use std::{
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let address = format!("127.0.0.1:{port}");
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_proxy-fork-cli"))
            .current_dir(temp.path())
            .env("XDG_CONFIG_HOME", temp.path().join("config"))
            .env("XDG_DATA_HOME", temp.path().join("data"))
            .args(["start-proxy", "--noca", "--listen", &address])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while std::net::TcpStream::connect(&address).is_err() {
        assert!(Instant::now() < deadline, "listener failed to start");
        assert!(child.0.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        Command::new("/bin/kill")
            .args(["-INT", &child.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "Ctrl+C failed to stop process");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(std::net::TcpStream::connect(&address).is_err());
    assert!(
        !temp
            .path()
            .join("data/proxy-fork/system-proxy-restore.toml")
            .exists()
    );
}
