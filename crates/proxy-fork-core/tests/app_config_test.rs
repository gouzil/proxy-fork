use proxy_fork_core::app_config::{AppConfig, FileConfig};

#[test]
fn config_round_trip_and_rejected_save_preserve_previous_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let cfg: FileConfig = toml::from_str("listen = '127.0.0.1:9898'\nnoca = true\n").unwrap();
    cfg.save(&path).unwrap();
    assert_eq!(
        FileConfig::load(&path)
            .unwrap()
            .runtime()
            .unwrap()
            .listen
            .port,
        9898
    );
    let before = fs_err::read(&path).unwrap();
    let mut invalid = cfg;
    invalid.listen = Some("127.0.0.1:65536".into());
    assert!(invalid.save(&path).is_err());
    assert_eq!(fs_err::read(&path).unwrap(), before);
}

#[tokio::test]
async fn invalid_rules_and_zero_cache_are_rejected_before_manager_replacement() {
    let mut cfg = AppConfig::default();
    cfg.proxy_manager.cache_size = 0;
    assert!(cfg.build_manager().await.is_err());
    let cfg: FileConfig = toml::from_str("noca = true\n[[proxy_manager.rules]]\nprotocol = 'ws'\nhost = 'example.com'\ntarget_host = 'localhost'\n").unwrap();
    assert!(cfg.runtime().is_err());
}
