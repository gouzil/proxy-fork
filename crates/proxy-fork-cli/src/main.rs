use clap::Parser;
use proxy_fork_cli::{args::CliArgs, run};

use tracing::error;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();
    // 设置日志级别
    let level = match args.global.debug {
        0 => "proxy_fork_cli=info,proxy_fork_core=info",
        1 => "proxy_fork_cli=debug,proxy_fork_core=debug",
        _ => "proxy_fork_cli=trace,proxy_fork_core=trace",
    };

    // 创建过滤器，只显示 proxy-fork 相关的日志
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    tracing_subscriber::fmt().with_env_filter(filter).init();

    match run(args).await {
        Ok(_) => {}
        Err(e) => {
            error!("Application error: {}", e);
        }
    }
}
