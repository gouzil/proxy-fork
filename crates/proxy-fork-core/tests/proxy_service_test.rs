use proxy_fork_core::{
    app_config::AppConfig,
    proxy_service::{ProxyService, ProxyServiceState},
};

#[tokio::test]
async fn service_restarts_and_rejects_duplicate_start() {
    let dir = tempfile::tempdir().unwrap();
    let service = ProxyService::new(dir.path().join("restore.toml"));
    let mut config = AppConfig {
        enable_ca: false,
        ..AppConfig::default()
    };
    config.listen.port = 0;
    assert_eq!(*service.subscribe().borrow(), ProxyServiceState::Stopped);
    let addr = service.start(config.clone()).await.unwrap();
    assert!(addr.port() > 0);
    assert!(tokio::net::TcpStream::connect(addr).await.is_ok());
    assert!(service.start(config.clone()).await.is_err());
    service.stop().await.unwrap();
    assert_eq!(*service.subscribe().borrow(), ProxyServiceState::Stopped);
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
    service.start(config).await.unwrap();
    service.stop().await.unwrap();
}

#[tokio::test]
async fn occupied_port_fails_without_changing_system_proxy() {
    let dir = tempfile::tempdir().unwrap();
    let service = ProxyService::new(dir.path().join("restore.toml"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = AppConfig {
        enable_ca: false,
        ..AppConfig::default()
    };
    config.listen.port = listener.local_addr().unwrap().port();
    assert!(service.start(config).await.is_err());
    assert!(matches!(
        *service.subscribe().borrow(),
        ProxyServiceState::Failed(_)
    ));
    assert!(!dir.path().join("restore.toml").exists());
}

#[tokio::test]
async fn saving_rules_is_all_or_nothing_for_disk_and_live_requests() {
    use proxy_fork_core::app_config::{FileConfig, RuleItem};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn backend(body: &'static str) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                assert!(stream.read(&mut request).await.unwrap() > 0);
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        (port, task)
    }
    let (old_port, old_server) = backend("old").await;
    let (new_port, new_server) = backend("new").await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let service = ProxyService::new(dir.path().join("restore.toml"));
    let mut config = AppConfig {
        enable_ca: false,
        ..AppConfig::default()
    };
    config.listen.port = 0;
    config.proxy_manager.rules.push(RuleItem {
        protocol: "http".into(),
        host: "service.test".into(),
        target_host: "127.0.0.1".into(),
        target_port: Some(old_port),
        ..RuleItem::default()
    });
    config.file_config().save(&path).unwrap();
    let original = fs_err::read(&path).unwrap();
    let address = service.start(config.clone()).await.unwrap();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{address}")).unwrap())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    assert_eq!(
        client
            .get("http://service.test/")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "old"
    );
    let mut changed = config.clone();
    changed.proxy_manager.rules[0].target_port = Some(new_port);
    changed.proxy_manager.rules[0].host = "re:[".into();
    assert!(
        service
            .save_and_replace_rules(&changed, path.clone())
            .await
            .is_err()
    );
    assert_eq!(fs_err::read(&path).unwrap(), original);
    assert_eq!(
        client
            .get("http://service.test/")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "old"
    );
    changed.proxy_manager.rules[0].host = "service.test".into();
    let blocked = dir.path().join("not-a-directory");
    fs_err::write(&blocked, b"file").unwrap();
    assert!(
        service
            .save_and_replace_rules(&changed, blocked.join("config.toml"))
            .await
            .is_err()
    );
    assert_eq!(fs_err::read(&path).unwrap(), original);
    assert_eq!(
        client
            .get("http://service.test/")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "old"
    );
    service
        .save_and_replace_rules(&changed, path.clone())
        .await
        .unwrap();
    assert_eq!(FileConfig::load(&path).unwrap(), changed.file_config());
    assert_eq!(
        client
            .get("http://service.test/")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "new"
    );
    service.stop().await.unwrap();
    old_server.abort();
    new_server.abort();
}
