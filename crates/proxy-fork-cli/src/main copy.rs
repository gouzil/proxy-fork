use http::{Uri, uri::Authority};
use hudsucker::{
    Body, HttpContext, HttpHandler, Proxy, RequestOrResponse, WebSocketContext, WebSocketHandler,
    certificate_authority::{CertificateAuthority, OpensslAuthority},
    hyper::{Request, Response},
    openssl::{hash::MessageDigest, pkey::PKey, x509::X509},
    rustls::{ServerConfig, crypto::aws_lc_rs},
    tokio_tungstenite::tungstenite::Message,
};
use std::{net::SocketAddr, sync::Arc};
use sysproxy::Sysproxy;
use tokio::sync::Mutex;
use tracing::{error, info, instrument::WithSubscriber};
use x509_parser::prelude::{X509Certificate, parse_x509_certificate};

async fn shutdown_signal(sysproxy: Arc<Mutex<Sysproxy>>) {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    info!("Press Ctrl+C to stop the proxy server...");
    let mut sysproxy = sysproxy.lock().await;
    sysproxy.enable = false;
    // sysproxy
    //     .set_system_proxy()
    //     .expect("Failed to set system proxy");
}

#[derive(Clone)]
struct LogHandler {
    data: i32,
}

struct NoCa;

impl CertificateAuthority for NoCa {
    async fn gen_server_config(&self, _authority: &Authority) -> Arc<ServerConfig> {
        unreachable!();
    }
}

impl HttpHandler for LogHandler {
    async fn handle_request(
        &mut self,
        _ctx: &HttpContext,
        mut req: Request<Body>,
    ) -> RequestOrResponse {
        // 解析出 uri
        let uri = req.uri().clone();
        // 从 uri 中解析出 scheme, host, port
        let scheme = uri.scheme_str().unwrap_or("http");
        let host = uri.host().unwrap_or("");
        let port = uri.port_u16();
        let new_uri_1 = {
            let include_port = match (scheme, port) {
                ("http", Some(80)) => true,
                ("https", Some(443)) => true,
                ("http", Some(443)) => true,
                (path_url, Some(i)) => {
                    println!("url: {path_url}  include_port: {i}");
                    false
                }
                (_, None) => false,
            };
            if include_port {
                Uri::builder()
                    .scheme(scheme)
                    .authority(format!("{}", host))
                    .path_and_query(uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/"))
                    .build()
                    .unwrap()
            } else {
                uri.clone()
            }
        };
        *req.uri_mut() = new_uri_1;
        if let Some(host) = req.uri().host()
            && host == "ragdev.oneai.art"
        {
            if req.uri().path().starts_with("/console/api") {
                // 构造新的 authority
                let new_authority = "localhost:5001";
                // 拆解 path_and_query
                let path_and_query = req
                    .uri()
                    .path_and_query()
                    .map(|pq| pq.as_str())
                    .unwrap_or("/");
                // 构造新的 Uri
                let new_uri = format!("http://{new_authority}{path_and_query}")
                    .parse()
                    .expect("Failed to parse new URI");
                *req.uri_mut() = new_uri;
            }
        }
        req.into()
    }

    async fn handle_response(&mut self, _ctx: &HttpContext, res: Response<Body>) -> Response<Body> {
        // println!("{:?}", res);
        res
    }

    // async fn should_intercept(&mut self, _ctx: &HttpContext, _req: &Request<Body>) -> bool {
    //     false
    // }
}

impl WebSocketHandler for LogHandler {
    async fn handle_message(&mut self, _ctx: &WebSocketContext, msg: Message) -> Option<Message> {
        // println!("{:?}", msg);
        Some(msg)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // 生成 CA 证书和私钥
    let mut ca_cert_file: Option<Vec<u8>> = None;
    for cert in rustls_native_certs::load_native_certs().expect("could not load platform certs") {
        match parse_x509_certificate(cert.as_ref()) {
            Ok((_, cert_)) => {
                let cn = cert_
                    .subject()
                    .iter_common_name()
                    .next()
                    .and_then(|cn| cn.as_str().ok());
                if let Some(cn) = cn
                    && cn == "mitmproxy"
                {
                    ca_cert_file = Some(cert.as_ref().to_vec());
                    break;
                }
            }
            Err(e) => eprintln!("error parsing certificate: {e}"),
        };
    }

    let private_key_bytes: &[u8] = include_bytes!("/Users/gouzi/.mitmproxy/mitmproxy-ca.pem");
    let private_key =
        PKey::private_key_from_pem(private_key_bytes).expect("Failed to parse private key");
    let ca_cert = if let Some(bytes) = ca_cert_file {
        X509::from_der(&bytes).expect("Failed to parse CA certificate")
    } else {
        panic!("No mitmproxy certificate found in platform certs");
    };

    let ca = OpensslAuthority::new(
        private_key,
        ca_cert,
        MessageDigest::sha256(),
        1_000,
        aws_lc_rs::default_provider(),
    );

    let sysproxy = Arc::new(Mutex::new(Sysproxy {
        enable: true,
        host: "127.0.0.1".into(),
        port: 7898,
        bypass: "localhost,127.0.0.1/8".into(),
    }));

    let proxy = Proxy::builder()
        .with_addr(SocketAddr::from(([127, 0, 0, 1], 7898)))
        // .with_ca(NoCa)
        .with_ca(ca)
        .with_rustls_client(aws_lc_rs::default_provider())
        .with_http_handler(LogHandler { data: 0 })
        .with_graceful_shutdown(shutdown_signal(sysproxy.clone()))
        .build()
        .expect("Failed to create proxy");

    // {
    //     let sysproxy_guard = sysproxy.lock().await;
    //     sysproxy_guard
    //         .set_system_proxy()
    //         .expect("Failed to set system proxy");
    // }

    if let Err(e) = proxy.start().await {
        error!("{}", e);
    }
}
