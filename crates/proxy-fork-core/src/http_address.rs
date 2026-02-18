use std::convert::TryFrom;

use derive_builder::Builder;
use http::Uri;
use std::error::Error;

use crate::{PatternMatcher, PatternType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Http,
    Https,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Http => write!(f, "http"),
            Protocol::Https => write!(f, "https"),
        }
    }
}

impl TryFrom<&Uri> for Protocol {
    type Error = ();

    fn try_from(uri: &Uri) -> Result<Self, Self::Error> {
        match uri
            .scheme_str()
            .unwrap_or("http")
            .to_ascii_lowercase()
            .as_str()
        {
            "http" => Ok(Protocol::Http),
            "https" => Ok(Protocol::Https),
            _ => Err(()),
        }
    }
}

/// 路径转换模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathTransformMode {
    /// 保留原始路径：只改变协议、主机和端口
    /// 例: https://example.com/api/users -> http://localhost:8080/api/users
    Preserve,

    /// 前缀拼接：将指定路径作为前缀拼接到原始路径前面
    /// 例: https://example.com/api/users -> http://localhost:8080/local/api/users
    Prepend,

    /// 前缀替换：将匹配的路径前缀替换为新的前缀
    /// 例: https://example.com/api/v1/users -> http://localhost:8080/api/v2/users
    Replace,
}

impl Default for PathTransformMode {
    fn default() -> Self {
        Self::Preserve
    }
}

impl std::str::FromStr for PathTransformMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "preserve" => Ok(PathTransformMode::Preserve),
            "prepend" => Ok(PathTransformMode::Prepend),
            "replace" => Ok(PathTransformMode::Replace),
            _ => Err(format!("Invalid PathTransformMode: {}", s)),
        }
    }
}

// 地址结构体
#[derive(Builder, Debug, Clone, PartialEq, Eq, Hash)]
#[builder(pattern = "owned")]
pub struct Address {
    pub protocol: Protocol,
    pub host: String,
    #[builder(default)]
    pub port: Option<u16>,
    #[builder(default)]
    pub path: Option<String>,
    /// 路径转换模式（默认为 Preserve）
    #[builder(default)]
    pub path_transform_mode: PathTransformMode,
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let authority = if let Some(port) = self.port {
            format!("{}:{}", self.host, port)
        } else {
            self.host.clone()
        };
        let path = self.path.as_deref().unwrap_or("/");
        write!(f, "{}://{}{}", self.protocol, authority, path)
    }
}

impl Address {
    /// 从 Uri 创建 Address
    pub fn from_uri(uri: &Uri) -> Result<Self, Box<dyn Error>> {
        let protocol = Protocol::try_from(uri).map_err(|_| "Invalid protocol")?;
        let host = uri.host().ok_or("Missing host")?.to_string();
        let port = uri.port_u16();
        let path = uri.path_and_query().map(|pq| pq.as_str().to_string());

        Ok(Self {
            protocol,
            host,
            port,
            path,
            path_transform_mode: PathTransformMode::default(),
        })
    }

    // /// 从生成器构建 Address，并将构建错误统一为 `Box<dyn Error>`
    // pub fn from_builder(builder: AddressBuilder) -> Result<Self, Box<dyn Error>> {
    //     builder.build().map_err(|e| Box::new(e) as Box<dyn Error>)
    // }

    // /// 将 Address 转换为 Uri
    // pub fn to_uri(&self) -> Result<Uri, http::Error> {
    //     let scheme = match self.protocol {
    //         Protocol::Http => "http",
    //         Protocol::Https => "https",
    //     };

    //     let authority = if let Some(port) = self.port {
    //         format!("{}:{}", self.host, port)
    //     } else {
    //         self.host.clone()
    //     };

    //     let path_and_query = self.path.as_deref().unwrap_or("/");

    //     Uri::builder()
    //         .scheme(scheme)
    //         .authority(authority)
    //         .path_and_query(path_and_query)
    //         .build()
    // }

    /// 将 Address 转换为 Uri，根据路径重写模式处理路径
    ///
    /// # 参数
    /// - `original_uri`: 原始请求的 URI
    /// - `matched_prefix`: 匹配到的路径前缀（从 pattern 中提取，仅在 Replace 模式下使用）
    ///
    /// # 路径转换模式
    ///
    /// ## PathTransformMode::Preserve (保留原始路径)
    /// ```ignore
    /// let target = Address {
    ///     path_transform_mode: PathTransformMode::Preserve,
    ///     path: None,
    ///     ...
    /// };
    /// // https://example.com/api/users -> http://localhost:8080/api/users
    /// ```
    ///
    /// ## PathTransformMode::Prepend (前缀拼接)
    /// ```ignore
    /// let target = Address {
    ///     path_transform_mode: PathTransformMode::Prepend,
    ///     path: Some("/local".to_string()),
    ///     ...
    /// };
    /// // https://example.com/api/users -> http://localhost:8080/local/api/users
    /// ```
    ///
    /// ## PathTransformMode::Replace (前缀替换)
    /// ```ignore
    /// let target = Address {
    ///     path_transform_mode: PathTransformMode::Replace,
    ///     path: Some("/api/v2".to_string()),
    ///     ...
    /// };
    /// // https://example.com/api/v1/users (matched_prefix="/api/v1")
    /// //   -> http://localhost:8080/api/v2/users
    /// ```
    pub fn to_uri_with_rewrite(
        &self,
        original_uri: &Uri,
        matched_prefix: Option<&str>,
    ) -> Result<Uri, http::Error> {
        let scheme = match self.protocol {
            Protocol::Http => "http",
            Protocol::Https => "https",
        };

        let authority = if let Some(port) = self.port {
            format!("{}:{}", self.host, port)
        } else {
            self.host.clone()
        };

        let original_path = original_uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        let path_and_query = match self.path_transform_mode {
            PathTransformMode::Preserve => {
                // 保留原始路径
                original_path.to_string()
            }
            PathTransformMode::Prepend => {
                // 前缀拼接
                if let Some(prefix) = &self.path {
                    let prefix_clean = prefix.trim_end_matches('/');
                    let original = if original_path.starts_with('/') {
                        original_path
                    } else {
                        "/"
                    };
                    format!("{}{}", prefix_clean, original)
                } else {
                    // 如果没有指定 path，回退到保留模式
                    original_path.to_string()
                }
            }
            PathTransformMode::Replace => {
                // 前缀替换
                if let (Some(new_prefix), Some(old_prefix)) = (&self.path, matched_prefix) {
                    let old_prefix_clean = old_prefix.trim_end_matches('*').trim_end_matches('/');

                    if original_path.starts_with(old_prefix_clean) {
                        // 提取匹配前缀之后的部分
                        let suffix = &original_path[old_prefix_clean.len()..];
                        let new_prefix_clean = new_prefix.trim_end_matches('/');

                        // 拼接新的路径
                        format!("{}{}", new_prefix_clean, suffix)
                    } else {
                        // 如果不匹配，保留原始路径
                        original_path.to_string()
                    }
                } else {
                    // 如果没有必要的参数，回退到保留模式
                    original_path.to_string()
                }
            }
        };

        Uri::builder()
            .scheme(scheme)
            .authority(authority)
            .path_and_query(path_and_query)
            .build()
    }
}

// 地址模式匹配器
#[derive(Builder, Debug, Clone)]
#[builder(pattern = "owned")]
pub struct AddressPattern {
    pub protocol: Protocol,
    #[builder(default)]
    pub port: Option<u16>,
    pub pattern_type: PatternType,
}

impl std::fmt::Display for AddressPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let port = self.port.map_or_else(String::new, |p| format!(":{}", p));
        let path = self
            .pattern_type
            .path
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default();
        write!(
            f,
            "{}://{}{}{}",
            self.protocol, self.pattern_type.host, port, path
        )
    }
}

impl AddressPattern {
    /// 从原始字符串创建地址模式
    pub fn new(
        protocol: Protocol,
        host: &str,
        port: Option<u16>,
        path: Option<&str>,
    ) -> Result<Self, regex::Error> {
        let host_strategy = PatternMatcher::from_str(host)?;
        let path_strategy = if let Some(p) = path {
            Some(PatternMatcher::from_str(p)?)
        } else {
            None
        };

        Ok(Self {
            protocol,
            port,
            pattern_type: PatternType {
                host: host_strategy,
                path: path_strategy,
            },
        })
    }

    /// 从生成器构建 AddressPattern，并将构建错误统一为 `Box<dyn Error>`
    pub fn from_builder(builder: AddressPatternBuilder) -> Result<Self, Box<dyn Error>> {
        builder.build().map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    /// 检查地址是否匹配此模式
    pub fn matches(&self, address: &Address) -> bool {
        // protocol 必须完全匹配
        if self.protocol != address.protocol {
            return false;
        }

        // port 匹配：如果模式指定了端口，则必须相等
        if let Some(pattern_port) = self.port {
            if address.port != Some(pattern_port) {
                return false;
            }
        }

        // host 匹配
        if !self.pattern_type.host.matches(&address.host) {
            return false;
        }

        // path 匹配
        match (&self.pattern_type.path, &address.path) {
            (None, _) => true, // 模式未约束 path
            (Some(strategy), Some(addr_path)) => strategy.matches(addr_path),
            (Some(_), None) => false, // 模式需要 path 但地址没有
        }
    }
}
