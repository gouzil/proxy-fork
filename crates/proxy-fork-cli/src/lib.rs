pub mod args;
pub mod commands;
pub mod config;
pub mod dirs;
use crate::{
    args::{CliArgs, Commands, GlobalConfigArgs, StartProxyArgs},
    config::load_start_proxy_config,
};
use anyhow::Result;

pub async fn run(CliArgs { command, global }: CliArgs) -> Result<()> {
    // 默认命令为 start-proxy
    let command = command.unwrap_or(Commands::StartProxy(StartProxyArgs::default()));

    // 执行对应命令
    match command {
        Commands::StartProxy(ref start_args) => start_proxy(start_args, &global).await,
        Commands::GenCa(ref gen_args) => commands::gen_ca::gen_ca(gen_args).await,
    }
}

async fn start_proxy(start_args: &StartProxyArgs, global: &GlobalConfigArgs) -> Result<()> {
    // 加载配置：CLI > CWD > 用户目录
    let cfg = load_start_proxy_config(&global, start_args)?;
    // 启动代理服务
    commands::start_proxy::start_proxy(&cfg).await
}
