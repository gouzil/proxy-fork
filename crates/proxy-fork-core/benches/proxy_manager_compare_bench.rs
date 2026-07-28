use std::{hint::black_box, io::ErrorKind, net::SocketAddr, sync::Arc};

use codspeed_criterion_compat::{Criterion, criterion_group, criterion_main};
use hudsucker::futures::{SinkExt, StreamExt};
use hudsucker::tokio_tungstenite::{
    accept_async, client_async, connect_async, tungstenite::Message,
};
use proxy_fork_core::{
    Address, AddressPattern, NoCa, PathTransformMode, Protocol, Proxy, ProxyHandlerBuilder,
    ProxyManager, rustls,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    runtime::Runtime,
    sync::RwLock,
    task::JoinHandle,
    time::Duration,
};

const PROXY_HTTP_HOST: &str = "bench-http.local";
const PROXY_WS_HOST: &str = "bench-ws.local";

struct BenchEnv {
    http_backend_addr: SocketAddr,
    ws_backend_addr: SocketAddr,
    proxy_addr: SocketAddr,
    _http_handle: JoinHandle<()>,
    _ws_handle: JoinHandle<()>,
    _proxy_handle: JoinHandle<()>,
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn run_http_backend(listener: TcpListener) {
    loop {
        let (socket, _) = listener.accept().await.expect("http backend accept failed");
        tokio::spawn(async move {
            let mut socket = socket;
            let mut read_buf = Vec::with_capacity(2048);
            let mut chunk = [0_u8; 1024];

            loop {
                let n = match socket.read(&mut chunk).await {
                    Ok(0) => return,
                    Ok(n) => n,
                    Err(_) => return,
                };
                read_buf.extend_from_slice(&chunk[..n]);

                while let Some(header_end) = find_header_end(&read_buf) {
                    let remaining = read_buf.split_off(header_end + 4);
                    let response =
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nOK";
                    if socket.write_all(response).await.is_err() {
                        return;
                    }
                    read_buf = remaining;
                }
            }
        });
    }
}

async fn run_ws_backend(listener: TcpListener) {
    loop {
        let (stream, _) = listener.accept().await.expect("ws backend accept failed");
        tokio::spawn(async move {
            let mut ws = match accept_async(stream).await {
                Ok(ws) => ws,
                Err(_) => return,
            };

            while let Some(message) = ws.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        if ws.send(Message::Text(text)).await.is_err() {
                            return;
                        }
                    }
                    Ok(Message::Close(_)) => return,
                    Ok(Message::Ping(payload)) => {
                        if ws.send(Message::Pong(payload)).await.is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => return,
                }
            }
        });
    }
}

async fn bind_listener_or_skip(addr: &str, role: &str) -> Option<TcpListener> {
    match TcpListener::bind(addr).await {
        Ok(listener) => Some(listener),
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            eprintln!("skipping benchmark: cannot bind {role} on {addr}: {e}");
            None
        }
        Err(e) => panic!("failed to bind {role} on {addr}: {e}"),
    }
}

async fn setup_env() -> Option<BenchEnv> {
    let http_listener = bind_listener_or_skip("127.0.0.1:0", "http backend").await?;
    let http_backend_addr = http_listener
        .local_addr()
        .expect("failed to get http backend addr");
    let http_handle = tokio::spawn(run_http_backend(http_listener));

    let ws_listener = bind_listener_or_skip("127.0.0.1:0", "ws backend").await?;
    let ws_backend_addr = ws_listener
        .local_addr()
        .expect("failed to get ws backend addr");
    let ws_handle = tokio::spawn(run_ws_backend(ws_listener));

    let mut manager =
        ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
            .expect("failed to build proxy manager");

    let http_pattern = AddressPattern::new(Protocol::Http, PROXY_HTTP_HOST, None, Some("/ping"))
        .expect("invalid http pattern");
    let http_target = Address {
        protocol: Protocol::Http,
        host: http_backend_addr.ip().to_string(),
        port: Some(http_backend_addr.port()),
        path: None,
        path_transform_mode: PathTransformMode::Preserve,
    };
    manager.add_rule(http_pattern, http_target).await;

    let ws_pattern = AddressPattern::new(Protocol::Http, PROXY_WS_HOST, None, Some("/ws"))
        .expect("invalid ws pattern");
    let ws_target = Address {
        protocol: Protocol::Http,
        host: ws_backend_addr.ip().to_string(),
        port: Some(ws_backend_addr.port()),
        path: None,
        path_transform_mode: PathTransformMode::Preserve,
    };
    manager.add_rule(ws_pattern, ws_target).await;

    let handler = ProxyHandlerBuilder::default()
        .proxy_manager(Arc::new(RwLock::new(manager)))
        // CONNECT WebSocket tunnel must be intercepted, same as real runtime behavior.
        .with_ca(true)
        .build()
        .expect("failed to build proxy handler");

    let proxy_listener = bind_listener_or_skip("127.0.0.1:0", "proxy").await?;
    let proxy_addr = proxy_listener
        .local_addr()
        .expect("failed to get proxy addr");

    let proxy = Proxy::builder()
        .with_listener(proxy_listener)
        .with_ca(NoCa)
        .with_rustls_connector(rustls::crypto::aws_lc_rs::default_provider())
        .with_http_handler(handler.clone())
        .with_websocket_handler(handler)
        .build()
        .expect("failed to build proxy");

    let proxy_handle = tokio::spawn(async move {
        proxy.start().await.expect("proxy failed");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    Some(BenchEnv {
        http_backend_addr,
        ws_backend_addr,
        proxy_addr,
        _http_handle: http_handle,
        _ws_handle: ws_handle,
        _proxy_handle: proxy_handle,
    })
}

async fn connect_ws_via_proxy(
    proxy_addr: SocketAddr,
) -> hudsucker::tokio_tungstenite::WebSocketStream<TcpStream> {
    let mut tunnel = TcpStream::connect(proxy_addr)
        .await
        .expect("failed to connect to proxy");

    let connect_request = format!(
        "CONNECT {host}:80 HTTP/1.1\r\nHost: {host}:80\r\nProxy-Connection: keep-alive\r\n\r\n",
        host = PROXY_WS_HOST
    );
    tunnel
        .write_all(connect_request.as_bytes())
        .await
        .expect("failed to write CONNECT request");

    let mut response = Vec::with_capacity(1024);
    let mut buf = [0_u8; 1024];
    loop {
        let n = tunnel
            .read(&mut buf)
            .await
            .expect("failed to read CONNECT response");
        assert!(n > 0, "proxy closed before CONNECT response");
        response.extend_from_slice(&buf[..n]);
        if find_header_end(&response).is_some() {
            break;
        }
    }
    let response_text = String::from_utf8_lossy(&response);
    assert!(
        response_text.starts_with("HTTP/1.1 200"),
        "unexpected CONNECT response: {response_text}"
    );

    let (ws, _) = client_async(format!("ws://{PROXY_WS_HOST}/ws"), tunnel)
        .await
        .expect("failed websocket handshake through proxy");
    ws
}

fn bench_http_roundtrip_direct_vs_proxy(c: &mut Criterion) {
    let mut group = c.benchmark_group("transport_http_roundtrip");
    let rt = Runtime::new().expect("failed to create tokio runtime");
    let Some(env) = rt.block_on(setup_env()) else {
        return;
    };

    let direct_client = reqwest::Client::builder()
        .build()
        .expect("failed to build direct http client");
    let proxy_client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{}", env.proxy_addr)).unwrap())
        .build()
        .expect("failed to build proxy http client");

    let direct_url = format!("http://{}/ping", env.http_backend_addr);
    let proxy_url = format!("http://{PROXY_HTTP_HOST}/ping");

    rt.block_on(async {
        let _ = direct_client
            .get(&direct_url)
            .send()
            .await
            .expect("direct http warmup failed");
        let _ = proxy_client
            .get(&proxy_url)
            .send()
            .await
            .expect("proxy http warmup failed");
    });

    group.bench_function("direct_http", |b| {
        b.iter(|| {
            rt.block_on(async {
                let resp = direct_client
                    .get(&direct_url)
                    .send()
                    .await
                    .expect("direct http request failed");
                let body = resp.bytes().await.expect("direct http body read failed");
                black_box(body);
            });
        });
    });

    group.bench_function("proxy_fork_http", |b| {
        b.iter(|| {
            rt.block_on(async {
                let resp = proxy_client
                    .get(&proxy_url)
                    .send()
                    .await
                    .expect("proxy http request failed");
                let body = resp.bytes().await.expect("proxy http body read failed");
                black_box(body);
            });
        });
    });

    group.finish();
}

fn bench_ws_message_roundtrip_direct_vs_proxy(c: &mut Criterion) {
    let mut group = c.benchmark_group("transport_ws_message_roundtrip");
    let rt = Runtime::new().expect("failed to create tokio runtime");
    let Some(env) = rt.block_on(setup_env()) else {
        return;
    };
    let direct_ws_url = format!("ws://{}/ws", env.ws_backend_addr);

    group.bench_function("direct_ws_message", |b| {
        let (mut ws, _) = rt
            .block_on(connect_async(&direct_ws_url))
            .expect("failed to connect direct websocket");
        let mut seq: u64 = 0;

        b.iter(|| {
            rt.block_on(async {
                let payload = format!("msg-{seq}");
                seq += 1;
                ws.send(Message::Text(payload.clone().into()))
                    .await
                    .expect("direct websocket send failed");
                let response = ws
                    .next()
                    .await
                    .expect("direct websocket closed")
                    .expect("direct websocket receive failed");
                match response {
                    Message::Text(text) => assert_eq!(text.as_str(), payload),
                    other => panic!("unexpected direct websocket message: {other:?}"),
                }
            });
        });

        let _ = rt.block_on(ws.close(None));
    });

    group.bench_function("proxy_fork_ws_message", |b| {
        let mut ws = rt.block_on(connect_ws_via_proxy(env.proxy_addr));
        let mut seq: u64 = 0;

        b.iter(|| {
            rt.block_on(async {
                let payload = format!("msg-{seq}");
                seq += 1;
                ws.send(Message::Text(payload.clone().into()))
                    .await
                    .expect("proxy websocket send failed");
                let response = ws
                    .next()
                    .await
                    .expect("proxy websocket closed")
                    .expect("proxy websocket receive failed");
                match response {
                    Message::Text(text) => assert_eq!(text.as_str(), payload),
                    other => panic!("unexpected proxy websocket message: {other:?}"),
                }
            });
        });

        let _ = rt.block_on(ws.close(None));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_http_roundtrip_direct_vs_proxy,
    bench_ws_message_roundtrip_direct_vs_proxy,
);
criterion_main!(benches);
