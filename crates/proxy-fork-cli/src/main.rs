use clap::Parser;
use proxy_fork_cli::{args::CliArgs, run};

use tracing::error;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    let args = CliArgs::parse();
    // 设置日志级别
    let level = match args.global.debug {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing_subscriber::fmt().with_max_level(level).init();
    match run(args).await {
        Ok(_) => {}
        Err(e) => {
            error!("Application error: {}", e);
        }
    }
}
