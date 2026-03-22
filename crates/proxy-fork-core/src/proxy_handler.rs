use std::sync::Arc;

use derive_builder::Builder;
use http::header::{CONNECTION, HOST, ORIGIN, UPGRADE};
use http::{Request, Uri};
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse, WebSocketContext, WebSocketHandler,
    tokio_tungstenite::tungstenite::Message,
};
use tokio::sync::RwLock;
use tracing::{debug, error};

use crate::ProxyManager;

#[derive(Clone, Builder)]
#[builder(pattern = "owned", name = "ProxyHandlerBuilder")]
pub struct ProxyHandler {
    // 代理管理器（必须）
    proxy_manager: Arc<RwLock<ProxyManager>>,
    #[builder(default = false)]
    with_ca: bool, // 是否启用自签名 CA 证书生成
}

impl ProxyHandler {
    fn header_as_str<'a>(req: &'a Request<Body>, name: &str) -> Option<&'a str> {
        req.headers().get(name).and_then(|v| v.to_str().ok())
    }

    fn is_websocket_upgrade(req: &Request<Body>) -> bool {
        let has_upgrade = req
            .headers()
            .get(CONNECTION)
            .and_then(|v| v.to_str().ok())
            .map(|v| {
                v.split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
            })
            .unwrap_or(false);

        let is_websocket = req
            .headers()
            .get(UPGRADE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("websocket"))
            .unwrap_or(false);

        has_upgrade && is_websocket
    }

    /// Remove websocket extensions before dialing upstream.
    ///
    /// `hudsucker`/`tokio-tungstenite` in current dependency graph does not
    /// consistently negotiate and decode all extension combinations end-to-end.
    /// Forwarding this header can cause upstream to send compressed frames and
    /// then fail with `Reserved bits are non-zero` while proxying.
    fn sanitize_websocket_upgrade(req: &mut Request<Body>) -> bool {
        req.headers_mut()
            .remove("sec-websocket-extensions")
            .is_some()
    }

    async fn rewrite_request_uri(&self, uri: &Uri) -> Option<Uri> {
        let manager = self.proxy_manager.read().await;
        let match_result = manager.find_target_with_match_info(uri).await?;

        match match_result
            .target
            .to_uri_with_rewrite(uri, match_result.matched_path_prefix.as_deref())
        {
            Ok(new_uri) => {
                debug!("Proxying {} -> {}", uri, new_uri);
                Some(new_uri)
            }
            Err(e) => {
                error!("Failed to convert target to URI: {}", e);
                None
            }
        }
    }

    fn should_intercept_connect(&self) -> bool {
        self.with_ca
    }
}

impl HttpHandler for ProxyHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        mut req: Request<Body>,
    ) -> RequestOrResponse {
        let is_ws_upgrade = Self::is_websocket_upgrade(&req);
        let original_uri = req.uri().clone();
        if is_ws_upgrade {
            let stripped_extensions = Self::sanitize_websocket_upgrade(&mut req);
            debug!(
                "WebSocket upgrade request: uri={}, host={:?}, origin={:?}, sec-websocket-protocol={:?}, sec-websocket-extensions={:?}, stripped_extensions={}",
                req.uri(),
                req.headers().get(HOST).and_then(|v| v.to_str().ok()),
                req.headers().get(ORIGIN).and_then(|v| v.to_str().ok()),
                Self::header_as_str(&req, "sec-websocket-protocol"),
                Self::header_as_str(&req, "sec-websocket-extensions"),
                stripped_extensions,
            );
        }

        if let Some(new_uri) = self.rewrite_request_uri(&original_uri).await {
            if is_ws_upgrade {
                debug!(
                    "WebSocket upstream rewrite: uri={} -> {}, host={:?}, origin={:?}",
                    original_uri,
                    new_uri,
                    req.headers().get(HOST).and_then(|v| v.to_str().ok()),
                    req.headers().get(ORIGIN).and_then(|v| v.to_str().ok()),
                );
            }
            *req.uri_mut() = new_uri;
        }

        req.into()
    }

    // 拦截所有 HTTPS 请求以进行证书生成
    async fn should_intercept(&mut self, _ctx: &HttpContext, _req: &Request<Body>) -> bool {
        // CONNECT 阶段通常拿不到完整 path，规则匹配可能不完整。
        // 开启 CA 时统一拦截，保证 HTTPS/WSS 后续请求都能进入 handle_request 做规则改写。
        self.should_intercept_connect()
    }
}

impl WebSocketHandler for ProxyHandler {
    async fn handle_message(&mut self, ctx: &WebSocketContext, msg: Message) -> Option<Message> {
        debug!("WebSocket message: {:?}", msg);
        match ctx {
            WebSocketContext::ClientToServer { src, dst, .. } => {
                debug!("WebSocket message from client: {:?}", msg);
                debug!("Client address: {}", src);
                debug!("Server address: {:?}", dst.host());
                Some(msg)
            }
            WebSocketContext::ServerToClient { src, dst, .. } => {
                debug!("WebSocket message from server: {:?}", msg);
                debug!("Server address: {}", src);
                debug!("Client address: {}", dst);
                Some(msg)
            }
        }
    }
}

#[cfg(test)]
#[path = "proxy_handler_test.rs"]
mod tests;
