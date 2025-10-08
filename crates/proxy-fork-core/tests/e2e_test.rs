use std::sync::Arc;

use hudsucker::Proxy;
use proxy_fork_core::{
    Address, AddressPattern, HttpProxyHandlerBuilder, NoCa, PatternMatcher, PatternType, Protocol,
    ProxyManager, rustls,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn test_end_to_end_proxy() {
    // Start a mock backend server
    let backend_addr = "127.0.0.1:0"; // 0 means assign a free port
    let backend_listener = tokio::net::TcpListener::bind(backend_addr).await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();

    // Spawn backend server
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = backend_listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0; 1024];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]);
                let response = if request.contains("GET /test") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, world!"
                } else if request.contains("GET /wild") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\nWildcard match"
                } else if request.contains("GET /regex") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nRegex match"
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"
                };
                socket.write_all(response.as_bytes()).await.unwrap();
            });
        }
    });

    // Create proxy manager with rules for different pattern types
    let config = ProxyManager::builder().cache_size(1000).build().unwrap();
    let mut proxy_manager = ProxyManager::from_config(config).unwrap();

    // Exact match rule
    let exact_pattern = AddressPattern {
        protocol: Protocol::Http,
        port: None,
        pattern_type: PatternType {
            host: PatternMatcher::Exact("example.com".to_string()),
            path: Some(PatternMatcher::Exact("/test".to_string())),
        },
    };
    let target = Address {
        protocol: Protocol::Http,
        host: backend_addr.ip().to_string(),
        port: Some(backend_addr.port()),
        path: None,
        path_transform_mode: proxy_fork_core::PathTransformMode::Preserve,
    };
    proxy_manager.add_rule(exact_pattern, target.clone()).await;

    // Wildcard match rule (*.example.com)
    let wildcard_pattern = AddressPattern {
        protocol: Protocol::Http,
        port: None,
        pattern_type: PatternType {
            host: PatternMatcher::Wildcard("*.example.com".to_string()),
            path: Some(PatternMatcher::Exact("/wild".to_string())),
        },
    };
    proxy_manager
        .add_rule(wildcard_pattern, target.clone())
        .await;

    // Regex match rule (re:example\..*)
    let regex_pattern = AddressPattern {
        protocol: Protocol::Http,
        port: None,
        pattern_type: PatternType {
            host: PatternMatcher::Regex {
                compiled: regex::Regex::new(r"example\..*").unwrap(),
                pattern: "re:example\\..*".to_string(),
            },
            path: Some(PatternMatcher::Exact("/regex".to_string())),
        },
    };
    proxy_manager.add_rule(regex_pattern, target).await;

    let proxy_manager = Arc::new(RwLock::new(proxy_manager));

    // Create proxy handler
    let handler = HttpProxyHandlerBuilder::default()
        .proxy_manager(proxy_manager)
        .build()
        .unwrap();

    // Start proxy server
    let proxy_addr: std::net::SocketAddr = "127.0.0.1:3128".parse().unwrap();
    let proxy = Proxy::builder()
        .with_addr(proxy_addr)
        .with_ca(NoCa)
        .with_rustls_connector(rustls::crypto::aws_lc_rs::default_provider())
        .with_http_handler(handler)
        .build()
        .unwrap();

    // Spawn proxy server
    let proxy_handle = tokio::spawn(async move {
        proxy.start().await.unwrap();
    });

    // Give servers time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create HTTP client with proxy
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{}", proxy_addr)).unwrap())
        .build()
        .unwrap();

    // Test exact match
    let response = timeout(
        Duration::from_secs(5),
        client.get("http://example.com/test").send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, "Hello, world!");

    // Test wildcard match (*.example.com)
    let response = timeout(
        Duration::from_secs(5),
        client.get("http://sub.example.com/wild").send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, "Wildcard match");

    // Test regex match (re:example\..*)
    let response = timeout(
        Duration::from_secs(5),
        client.get("http://example.org/regex").send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, "Regex match");

    // Stop proxy
    proxy_handle.abort();
}
