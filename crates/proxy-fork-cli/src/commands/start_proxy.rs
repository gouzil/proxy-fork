use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use proxy_fork_core::{
    Address, AddressBuilder, AddressPattern, CaEnum, CertInput, NoCa, PathTransformMode, Protocol,
    Proxy, ProxyHandlerBuilder, ProxyManager, load_ca_from_sources, rustls::crypto::aws_lc_rs,
};
use sysproxy::Sysproxy;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};

use crate::{
    args::RuleItem,
    config::AppConfig,
    dirs::{APP_NAME, default_cert_path, default_private_key_path},
};

async fn shutdown_signal(sysproxy: Option<Arc<Mutex<Sysproxy>>>) {
    // 支持两种关闭方式，一种是 Ctrl+C，另一种是通过 channel 发送关闭信号
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received again, shutting down immediately.");
        }
    }
    if let Some(ref sysproxy_arc) = sysproxy {
        let mut sysproxy_guard = sysproxy_arc.lock().await;
        sysproxy_guard.enable = false;
        if let Err(e) = sysproxy_guard.set_system_proxy() {
            error!("Failed to disable system proxy: {}", e);
        }
    }
}

fn rule_item_to_runtime(r: &RuleItem) -> Option<(AddressPattern, Address)> {
    let protocol = match r.protocol.as_str() {
        "http" => Protocol::Http,
        "https" => Protocol::Https,
        _ => return None,
    };
    let pattern = AddressPattern::new(protocol, &r.host, r.port, r.path.as_deref()).ok()?;

    let target_protocol = match r.target_protocol.as_deref().unwrap_or("http") {
        "http" => Protocol::Http,
        "https" => Protocol::Https,
        _ => Protocol::Http,
    };

    let mut builder = AddressBuilder::default()
        .protocol(target_protocol)
        .host(r.target_host.clone())
        .port(r.target_port);

    builder = if let Some(mode) = r.path_transform.as_deref() {
        let mode = PathTransformMode::from_str(mode).unwrap_or_default();
        builder.path_transform_mode(mode)
    } else {
        builder
    };

    builder = if let Some(p) = r.target_path.as_ref() {
        builder.path(Some(p.clone()))
    } else {
        builder
    };

    Some((pattern, builder.build().ok()?))
}

pub(crate) async fn start_proxy(cfg: &AppConfig) -> anyhow::Result<()> {
    let ca = if cfg.enable_ca {
        // 统一加载 CA 证书和私钥（优先使用系统证书，私钥从本地 PEM 文件读取）
        match (&cfg.ca_cert, &cfg.ca_key) {
            (Some(cert), Some(key)) => CaEnum::Openssl(
                load_ca_from_sources(
                    CertInput::File(cert.to_string_lossy().as_ref()),
                    CertInput::File(key.to_string_lossy().as_ref()),
                )
                .expect("Failed to load CA certificate and private key"),
            ),
            // 允许只提供证书名时尝试系统证书 + 文件 key
            (None, Some(key)) => CaEnum::Openssl(
                load_ca_from_sources(
                    CertInput::System(APP_NAME),
                    CertInput::File(key.to_string_lossy().as_ref()),
                )
                .expect("Failed to load CA certificate and private key"),
            ),
            _ => CaEnum::Openssl(
                load_ca_from_sources(
                    CertInput::File(default_cert_path().as_ref().unwrap().to_str().unwrap()),
                    CertInput::File(
                        default_private_key_path()
                            .as_ref()
                            .unwrap()
                            .to_str()
                            .unwrap(),
                    ),
                )
                .expect("Failed to load CA certificate and private key"),
            ),
        }
    } else {
        CaEnum::None(NoCa)
    };

    // 初始化 proxy manager
    let mut proxy_manager = ProxyManager::from_config(
        ProxyManager::builder()
            .cache_size(cfg.proxy_manager.cache_size)
            .build()
            .unwrap(),
    )
    .expect("Failed to construct ProxyManager from config");

    // 从配置添加规则
    for r in cfg.proxy_manager.rules.iter() {
        if let Some((pattern, target)) = rule_item_to_runtime(r) {
            proxy_manager.add_rule(pattern, target).await;
        } else {
            error!("invalid rule in config, skipped: {:?}", r);
        }
    }

    // 创建共享的 proxy manager
    let proxy_manager_arc = Arc::new(RwLock::new(proxy_manager));

    // 初始化单个 proxy handler（共享同一个 proxy manager）
    let proxy_handler = Arc::new(
        ProxyHandlerBuilder::default()
            .proxy_manager(proxy_manager_arc.clone())
            .with_ca(cfg.enable_ca)
            .build()
            .unwrap(),
    );

    // 系统代理配置
    let sysproxy = if cfg.enable_sysproxy {
        Some(Arc::new(Mutex::new(Sysproxy {
            enable: true,
            host: cfg.listen.host.clone(),
            port: cfg.listen.port,
            bypass: "localhost,127.0.0.1/8".into(),
        })))
    } else {
        None
    };

    // 如果启用系统代理，则设置
    if let Some(ref sysproxy_arc) = sysproxy {
        let sysproxy_guard = sysproxy_arc.lock().await;
        if let Err(e) = sysproxy_guard.set_system_proxy() {
            error!("Failed to set system proxy: {}", e);
        }
    }

    let listen_ip = if cfg.listen.host == "localhost" {
        IpAddr::from([127, 0, 0, 1])
    } else {
        cfg.listen
            .host
            .parse()
            .unwrap_or(IpAddr::from([127, 0, 0, 1]))
    };
    let proxy = Proxy::builder()
        .with_addr(SocketAddr::from((listen_ip, cfg.listen.port)))
        // .with_ca(NoCa)
        .with_ca(ca)
        .with_rustls_connector(aws_lc_rs::default_provider())
        .with_http_handler((*proxy_handler).clone())
        .with_websocket_handler((*proxy_handler).clone())
        .with_graceful_shutdown(shutdown_signal(sysproxy.clone()))
        .build()
        .expect("Failed to create proxy");

    print_server_info(cfg, proxy_manager_arc).await?;
    info!("Proxy service startup complete. Ready to accept requests.");
    info!("Press Ctrl+C to stop the proxy service.");

    if let Err(e) = proxy.start().await {
        error!("{}", e);
    }
    Ok(())
}

async fn print_server_info(
    cfg: &AppConfig,
    proxy_manager: Arc<RwLock<ProxyManager>>,
) -> anyhow::Result<()> {
    let listen_ip = if cfg.listen.host == "localhost" {
        IpAddr::from([127, 0, 0, 1])
    } else {
        cfg.listen
            .host
            .parse()
            .unwrap_or(IpAddr::from([127, 0, 0, 1]))
    };
    info!(
        "Proxy server listening on {}:{}",
        listen_ip, cfg.listen.port
    );
    if cfg.enable_sysproxy {
        info!("System proxy is enabled");
    } else {
        info!("System proxy is disabled");
    }
    if cfg.enable_ca {
        info!("CA is enabled");
    } else {
        info!("CA is disabled");
    }

    // 打印所有规则（使用 ProxyManager 的 Display 实现）
    let manager = proxy_manager.read().await;
    info!("{}", manager);

    Ok(())
}
