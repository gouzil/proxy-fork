use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// 全局配置参数
#[derive(Parser, Debug, Clone, Default)]
pub struct GlobalConfigArgs {
    /// 指定配置文件路径（TOML）。如果提供，则优先使用此路径的配置。
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// 启用调试日志
    #[arg(long, short, action = clap::ArgAction::Count)]
    pub debug: u8,
}

/// CLI 参数定义
#[derive(Parser, Debug, Clone)]
#[command(
    name = "proxy-fork",
    version,
    about = "A flexible HTTP(S) proxy with dynamic rules"
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub global: GlobalConfigArgs,
}

/// 子命令定义
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// 启动代理服务器
    StartProxy(StartProxyArgs),
    /// 生成 CA 证书
    GenCa(GenCaArgs),
}

/// 启动代理的参数
#[derive(Parser, Debug, Clone, Default)]
pub struct StartProxyArgs {
    /// CA 证书与私钥位置（可覆盖文件中的 cert/key）。
    #[arg(long, value_name = "CERT_FILE")]
    pub ca_cert: Option<PathBuf>,
    #[arg(long, value_name = "KEY_FILE")]
    pub ca_key: Option<PathBuf>,

    /// 监听地址与端口，例如 127.0.0.1:7898（可选）。
    #[arg(long, value_name = "HOST:PORT")]
    pub listen: Option<String>,

    /// 通过 CLI 添加规则，可多次传入；格式：
    /// protocol=http|https,host=example.com[,path=/api/*][,port=443],target_host=127.0.0.1[,target_port=8080][,target_protocol=http|https][,path_transform=preserve|prepend|replace][,target_path=/new]
    #[arg(long = "rule", value_name = "RULE", value_parser = parse_rule_arg)]
    pub rules: Vec<RuleItem>,

    /// 启用系统代理
    #[arg(long, default_value_t = false, action = clap::ArgAction::SetTrue)]
    pub enable_sysproxy: bool,

    /// 禁用 CA 证书（无证书模式）
    #[arg(long, default_value_t = false, action = clap::ArgAction::SetTrue)]
    pub noca: bool,
}

/// 生成 CA 证书的参数
#[derive(Parser, Debug, Clone, Default)]
pub struct GenCaArgs {
    /// CA 证书输出位置
    #[arg(long, value_name = "CERT_FILE")]
    pub ca_cert: Option<PathBuf>,
    /// CA 私钥输出位置
    #[arg(long, value_name = "KEY_FILE")]
    pub ca_key: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RuleItem {
    /// protocol: "http" | "https"
    pub protocol: String,
    /// 需要代理的域名（支持通配符或正则规则）
    pub host: String,
    /// 需要代理的路径（可选，支持通配符或正则规则）
    pub path: Option<String>,
    /// 可选端口
    pub port: Option<u16>,

    /// 目标地址
    pub target_protocol: Option<String>,
    pub target_host: String,
    pub target_port: Option<u16>,
    /// 路径重写模式：preserve|prepend|replace
    pub path_transform: Option<String>,
    /// 若为 prepend/replace，新的路径前缀
    pub target_path: Option<String>,
}

pub(crate) fn parse_rule_arg(s: &str) -> Result<RuleItem, String> {
    // 解析 key=value, 用逗号分隔
    let mut map = std::collections::HashMap::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((k, v)) = part.split_once('=') else {
            return Err(format!("invalid segment: {}", part));
        };
        map.insert(k.trim().to_lowercase(), v.trim().to_string());
    }

    let get = |k: &str| map.get(k).cloned();
    let required = |k: &str| get(k).ok_or_else(|| format!("missing required key: {}", k));

    let protocol = required("protocol")?;
    if protocol != "http" && protocol != "https" {
        return Err("protocol must be http or https".into());
    }
    let host = required("host")?;
    let target_host = required("target_host")?;

    let path = get("path");
    let port = get("port").and_then(|v| v.parse::<u16>().ok());
    let target_protocol = get("target_protocol");
    let target_port = get("target_port").and_then(|v| v.parse::<u16>().ok());
    let path_transform = get("path_transform");
    let target_path = get("target_path");

    Ok(RuleItem {
        protocol,
        host,
        path,
        port,
        target_protocol,
        target_host,
        target_port,
        path_transform,
        target_path,
    })
}

#[cfg(test)]
mod tests {
    use crate::args::parse_rule_arg;

    #[test]
    fn test_parse_rule_arg_minimal() {
        let rule = parse_rule_arg("protocol=https,host=example.com,target_host=127.0.0.1").unwrap();
        assert_eq!(rule.protocol, "https");
        assert_eq!(rule.host, "example.com");
        assert_eq!(rule.target_host, "127.0.0.1");
        assert!(rule.path.is_none());
        assert!(rule.port.is_none());
    }
}
