use hudsucker::futures::{SinkExt, StreamExt};
use hudsucker::tokio_tungstenite::{accept_async, client_async, tungstenite::Message};
use proxy_fork_core::{app_config::AppConfig, proxy_service::ProxyService};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

async fn connect_tunnel(
    proxy: std::net::SocketAddr,
    target: std::net::SocketAddr,
) -> BufReader<TcpStream> {
    let mut client = BufReader::new(TcpStream::connect(proxy).await.unwrap());
    client
        .get_mut()
        .write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    loop {
        let mut line = String::new();
        timeout(Duration::from_secs(2), client.read_line(&mut line))
            .await
            .expect("CONNECT handshake timed out")
            .unwrap();
        if line == "\r\n" {
            break;
        }
        assert!(!line.is_empty());
    }
    client
}

#[tokio::test]
async fn stop_bounds_an_unfinished_http_response() {
    let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = backend.local_addr().unwrap();
    let backend = tokio::spawn(async move {
        let (mut stream, _) = backend.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\n\r\nx")
            .await
            .unwrap();
        std::future::pending::<()>().await;
    });
    let dir = tempfile::tempdir().unwrap();
    let service = ProxyService::new(dir.path().join("restore.toml"));
    let mut config = AppConfig {
        enable_ca: false,
        ..AppConfig::default()
    };
    config.listen.port = 0;
    let proxy = service.start(config).await.unwrap();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{proxy}")).unwrap())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let mut response = client
        .get(format!("http://{address}/"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.chunk().await.unwrap().unwrap(), "x");
    timeout(Duration::from_secs(4), service.stop())
        .await
        .expect("stop hung on an active response")
        .unwrap();
    assert!(TcpStream::connect(proxy).await.is_err());
    assert!(response.chunk().await.is_err());
    backend.abort();
}

#[tokio::test]
async fn stop_closes_an_active_connect_tunnel() {
    let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = backend.local_addr().unwrap();
    let backend = tokio::spawn(async move {
        let (mut stream, _) = backend.accept().await.unwrap();
        let mut ping = [0; 4];
        stream.read_exact(&mut ping).await.unwrap();
        stream.write_all(&ping).await.unwrap();
        std::future::pending::<()>().await;
    });
    let dir = tempfile::tempdir().unwrap();
    let service = ProxyService::new(dir.path().join("restore.toml"));
    let mut config = AppConfig {
        enable_ca: false,
        ..AppConfig::default()
    };
    config.listen.port = 0;
    let proxy = service.start(config).await.unwrap();
    let mut client = connect_tunnel(proxy, address).await;
    client.get_mut().write_all(b"ping").await.unwrap();
    let mut ping = [0; 4];
    timeout(Duration::from_secs(2), client.read_exact(&mut ping))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&ping, b"ping");
    timeout(Duration::from_secs(4), service.stop())
        .await
        .unwrap()
        .unwrap();
    let result = timeout(Duration::from_secs(1), client.read(&mut ping))
        .await
        .expect("tunnel remained open after stop");
    assert!(matches!(result, Ok(0) | Err(_)));
    backend.abort();
}

#[tokio::test]
async fn stop_closes_an_active_websocket() {
    let backend = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = backend.local_addr().unwrap();
    let backend = tokio::spawn(async move {
        let (stream, _) = backend.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        while let Some(Ok(message)) = websocket.next().await {
            if websocket.send(message).await.is_err() {
                break;
            }
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let service = ProxyService::new(dir.path().join("restore.toml"));
    let ca = proxy_fork_core::generate_ca(
        &dir.path().join("ca.pem"),
        &dir.path().join("ca.key"),
        false,
    )
    .unwrap();
    let mut config = AppConfig {
        ca_cert: Some(ca.cert_path),
        ca_key: Some(ca.key_path),
        ..AppConfig::default()
    };
    config.listen.port = 0;
    let proxy = service.start(config).await.unwrap();
    let tunnel = connect_tunnel(proxy, address).await;
    let (mut websocket, _) = timeout(
        Duration::from_secs(2),
        client_async(format!("ws://{address}/"), tunnel.into_inner()),
    )
    .await
    .expect("WebSocket handshake timed out")
    .unwrap();
    websocket.send(Message::Text("ping".into())).await.unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), websocket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        Message::Text("ping".into())
    );
    timeout(Duration::from_secs(4), service.stop())
        .await
        .unwrap()
        .unwrap();
    let result = timeout(Duration::from_secs(1), websocket.next())
        .await
        .expect("WebSocket remained open after stop");
    assert!(matches!(
        result,
        None | Some(Err(_)) | Some(Ok(Message::Close(_)))
    ));
    backend.abort();
}
