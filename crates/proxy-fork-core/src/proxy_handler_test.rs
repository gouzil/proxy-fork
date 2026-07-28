use std::sync::Arc;

use http::Request;
use http::Uri;
use tokio::sync::RwLock;

use super::*;
use crate::{Address, AddressPattern, PathTransformMode, Protocol, ProxyManager};

#[tokio::test]
async fn websocket_request_can_be_rewritten_by_http_handler() {
    let mut manager =
        ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
            .expect("Failed to construct ProxyManager from config");

    let pattern =
        AddressPattern::new(Protocol::Http, "ws.example.com", None, Some("/socket/*")).unwrap();
    let target = Address {
        protocol: Protocol::Http,
        host: "localhost".to_string(),
        port: Some(5002),
        path: None,
        path_transform_mode: PathTransformMode::Preserve,
    };
    manager.add_rule(pattern, target).await;

    let manager = Arc::new(RwLock::new(manager));
    let handler = ProxyHandlerBuilder::default()
        .proxy_manager(manager)
        .with_ca(true)
        .build()
        .unwrap();

    let original_uri: Uri = "ws://ws.example.com/socket/chat?token=1".parse().unwrap();
    let rewritten = handler.rewrite_request_uri(&original_uri).await.unwrap();

    assert_eq!(
        rewritten.to_string(),
        "http://localhost:5002/socket/chat?token=1"
    );
}

#[tokio::test]
async fn should_intercept_returns_true_when_ca_enabled() {
    let manager =
        ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
            .expect("Failed to construct ProxyManager from config");

    let manager = Arc::new(RwLock::new(manager));
    let handler = ProxyHandlerBuilder::default()
        .proxy_manager(manager)
        .with_ca(true)
        .build()
        .unwrap();

    assert!(handler.should_intercept_connect());
}

#[tokio::test]
async fn should_intercept_returns_false_when_ca_disabled() {
    let manager =
        ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
            .expect("Failed to construct ProxyManager from config");

    let manager = Arc::new(RwLock::new(manager));
    let handler = ProxyHandlerBuilder::default()
        .proxy_manager(manager)
        .with_ca(false)
        .build()
        .unwrap();

    assert!(!handler.should_intercept_connect());
}

#[test]
fn websocket_extensions_are_stripped_before_upstream() {
    let mut req = Request::builder()
        .method("GET")
        .uri("https://echo.websocket.org:443/")
        .header(CONNECTION, "Upgrade")
        .header(UPGRADE, "websocket")
        .header(
            "sec-websocket-extensions",
            "permessage-deflate; client_max_window_bits",
        )
        .body(Body::empty())
        .unwrap();

    assert!(ProxyHandler::is_websocket_upgrade(&req));
    assert!(ProxyHandler::sanitize_websocket_upgrade(&mut req));
    assert!(req.headers().get("sec-websocket-extensions").is_none());
}
