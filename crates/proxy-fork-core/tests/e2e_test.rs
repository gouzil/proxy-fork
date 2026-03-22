use std::io::ErrorKind;
use std::sync::Arc;

use hudsucker::Proxy;
use hudsucker::futures::{SinkExt, StreamExt};
use hudsucker::tokio_tungstenite::{
    accept_hdr_async, client_async,
    tungstenite::{
        ClientRequestBuilder, Message,
        handshake::server::{Request as WsRequest, Response as WsResponse},
    },
};
use proxy_fork_core::{
    Address, AddressPattern, NoCa, PatternMatcher, PatternType, Protocol, ProxyHandlerBuilder,
    ProxyManager, rustls,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};

async fn bind_or_skip(addr: &str, test_name: &str) -> Option<TcpListener> {
    match TcpListener::bind(addr).await {
        Ok(listener) => Some(listener),
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            eprintln!(
                "skipping {}: cannot bind {} in current environment: {}",
                test_name, addr, e
            );
            None
        }
        Err(e) => panic!("{}: failed to bind {}: {}", test_name, addr, e),
    }
}

#[tokio::test]
async fn test_end_to_end_proxy() {
    // Start a mock backend server
    let backend_addr = "127.0.0.1:0"; // 0 means assign a free port
    let Some(backend_listener) = bind_or_skip(backend_addr, "test_end_to_end_proxy").await else {
        return;
    };
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
    let handler = ProxyHandlerBuilder::default()
        .proxy_manager(proxy_manager)
        .build()
        .unwrap();

    // Start proxy server
    let Some(proxy_listener) = bind_or_skip("127.0.0.1:0", "test_end_to_end_proxy").await else {
        return;
    };
    let proxy_addr = proxy_listener.local_addr().unwrap();
    let proxy = Proxy::builder()
        .with_listener(proxy_listener)
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

#[tokio::test]
async fn test_end_to_end_websocket_proxy() {
    // Start a mock websocket backend server
    let Some(backend_listener) =
        bind_or_skip("127.0.0.1:0", "test_end_to_end_websocket_proxy").await
    else {
        return;
    };
    let backend_addr = backend_listener.local_addr().unwrap();

    let backend_handle = tokio::spawn(async move {
        loop {
            let (stream, _) = backend_listener.accept().await.unwrap();
            tokio::spawn(async move {
                let callback = |req: &WsRequest, mut response: WsResponse| {
                    // Mirror first requested subprotocol to satisfy client-side validation.
                    if let Some(requested) = req
                        .headers()
                        .get("Sec-WebSocket-Protocol")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.split(',').map(str::trim).find(|p| !p.is_empty()))
                    {
                        if let Ok(value) = requested.parse() {
                            response
                                .headers_mut()
                                .insert("Sec-WebSocket-Protocol", value);
                        }
                    }
                    Ok(response)
                };

                let mut websocket = match accept_hdr_async(stream, callback).await {
                    Ok(ws) => ws,
                    Err(_) => return,
                };

                while let Some(message) = websocket.next().await {
                    match message {
                        Ok(Message::Text(text)) => {
                            if websocket
                                .send(Message::Text(format!("echo:{text}").into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(Message::Ping(payload)) => {
                            if websocket.send(Message::Pong(payload)).await.is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            });
        }
    });

    // Create proxy manager with websocket rule
    let config = ProxyManager::builder().cache_size(1000).build().unwrap();
    let mut proxy_manager = ProxyManager::from_config(config).unwrap();

    let ws_pattern = AddressPattern::new(Protocol::Http, "ws.example.com", None, Some("/ws"))
        .expect("invalid websocket pattern");
    let ws_target = Address {
        protocol: Protocol::Http,
        host: backend_addr.ip().to_string(),
        port: Some(backend_addr.port()),
        path: None,
        path_transform_mode: proxy_fork_core::PathTransformMode::Preserve,
    };
    proxy_manager.add_rule(ws_pattern, ws_target).await;

    let proxy_manager = Arc::new(RwLock::new(proxy_manager));

    // with_ca(true) is required so CONNECT websocket tunnels are intercepted
    let handler = ProxyHandlerBuilder::default()
        .proxy_manager(proxy_manager)
        .with_ca(true)
        .build()
        .unwrap();

    // Start proxy server on an ephemeral port
    let Some(proxy_listener) = bind_or_skip("127.0.0.1:0", "test_end_to_end_websocket_proxy").await
    else {
        return;
    };
    let proxy_addr = proxy_listener.local_addr().unwrap();
    let proxy = Proxy::builder()
        .with_listener(proxy_listener)
        .with_ca(NoCa)
        .with_rustls_connector(rustls::crypto::aws_lc_rs::default_provider())
        .with_http_handler(handler.clone())
        .with_websocket_handler(handler)
        .build()
        .unwrap();

    let proxy_handle = tokio::spawn(async move {
        proxy.start().await.unwrap();
    });

    // Give servers time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 1) Open a CONNECT tunnel to target host through proxy
    let mut tunnel = timeout(Duration::from_secs(5), TcpStream::connect(proxy_addr))
        .await
        .unwrap()
        .unwrap();

    let connect_request = b"CONNECT ws.example.com:80 HTTP/1.1\r\nHost: ws.example.com:80\r\nProxy-Connection: keep-alive\r\n\r\n";
    timeout(Duration::from_secs(5), tunnel.write_all(connect_request))
        .await
        .unwrap()
        .unwrap();

    let mut connect_response = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = timeout(Duration::from_secs(5), tunnel.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(n > 0, "proxy closed tunnel before CONNECT response");
        connect_response.extend_from_slice(&buf[..n]);
        if connect_response.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let connect_response_text = String::from_utf8_lossy(&connect_response);
    assert!(
        connect_response_text.starts_with("HTTP/1.1 200"),
        "unexpected CONNECT response: {connect_response_text}"
    );

    // 2) Perform websocket handshake and verify message forwarding
    let ws_request = ClientRequestBuilder::new("ws://ws.example.com/ws".parse().unwrap())
        .with_sub_protocol("auth-token");

    let (mut websocket, ws_response) =
        timeout(Duration::from_secs(5), client_async(ws_request, tunnel))
            .await
            .unwrap()
            .unwrap();

    assert_eq!(
        ws_response
            .headers()
            .get("Sec-WebSocket-Protocol")
            .and_then(|v| v.to_str().ok()),
        Some("auth-token")
    );

    timeout(
        Duration::from_secs(5),
        websocket.send(Message::Text("hello-proxy".into())),
    )
    .await
    .unwrap()
    .unwrap();

    let first_message = timeout(Duration::from_secs(5), websocket.next())
        .await
        .unwrap()
        .expect("websocket closed unexpectedly")
        .expect("websocket read error");

    match first_message {
        Message::Text(text) => assert_eq!(text.to_string(), "echo:hello-proxy"),
        other => panic!("unexpected websocket message: {other:?}"),
    }

    timeout(Duration::from_secs(5), websocket.close(None))
        .await
        .unwrap()
        .unwrap();

    proxy_handle.abort();
    backend_handle.abort();
}
