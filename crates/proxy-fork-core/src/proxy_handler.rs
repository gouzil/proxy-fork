use std::sync::Arc;

use derive_builder::Builder;
use http::Request;
use hudsucker::{
    Body, HttpContext, HttpHandler, RequestOrResponse, WebSocketContext, WebSocketHandler,
    tokio_tungstenite::tungstenite::Message,
};
use tokio::sync::RwLock;
use tracing::{debug, error};

use crate::{Protocol, ProxyManager};

#[derive(Clone, Builder)]
#[builder(pattern = "owned", name = "ProxyHandlerBuilder")]
pub struct ProxyHandler {
    // 代理管理器（必须）
    proxy_manager: Arc<RwLock<ProxyManager>>,
    #[builder(default = false)]
    with_ca: bool, // 是否启用自签名 CA 证书生成
}

impl HttpHandler for ProxyHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        mut req: Request<Body>,
    ) -> RequestOrResponse {
        // 查找匹配的代理规则（包含匹配信息）
        let manager = self.proxy_manager.read().await;
        if let Some(match_result) = manager.find_target_with_match_info(req.uri()).await {
            // 使用新的路径重写方法，根据 path_rewrite_mode 处理路径
            match match_result
                .target
                .to_uri_with_rewrite(req.uri(), match_result.matched_path_prefix.as_deref())
            {
                Ok(new_uri) => {
                    debug!("Proxying {} -> {}", req.uri(), new_uri);
                    *req.uri_mut() = new_uri;
                }
                Err(e) => {
                    error!("Failed to convert target to URI: {}", e);
                }
            }
        }

        req.into()
    }

    // 拦截所有 HTTPS 请求以进行证书生成
    async fn should_intercept(&mut self, _ctx: &HttpContext, req: &Request<Body>) -> bool {
        if !self.with_ca {
            return false; // 如果未启用 CA，直接返回 false
        }
        let manager = self.proxy_manager.read().await;
        if let Some(target) = manager.find_target(req.uri()).await {
            match target.protocol {
                Protocol::Https => true,
                _ => false,
            }
        } else {
            true // 默认拦截所有 HTTPS 请求
        }
    }
}

impl WebSocketHandler for ProxyHandler {
    async fn handle_message(&mut self, ctx: &WebSocketContext, msg: Message) -> Option<Message> {
        debug!("WebSocket message: {:?}", msg);
        match ctx {
            WebSocketContext::ClientToServer { src, dst, .. } => {
                // 处理来自客户端的消息
                debug!("WebSocket message from client: {:?}", msg);
                debug!("Client address: {}", src);
                debug!("Server address: {:?}", dst.host());
                Some(msg) // 这里简单地将消息转发到服务器
            }
            WebSocketContext::ServerToClient { src, dst, .. } => {
                // 处理来自服务器的消息
                debug!("WebSocket message from server: {:?}", msg);
                debug!("Server address: {}", src);
                debug!("Client address: {}", dst);
                Some(msg) // 这里简单地将消息转发到客户端
            }
        }
    }
}
