use crate::{
    Address, AddressPattern, PatternMatcher, Protocol, ProxyStatsSnapshot, stats_impl::ProxyStats,
};
use derive_builder::Builder;
use std::collections::HashMap;
use std::num::NonZeroUsize;

use http::Uri;
use lru::LruCache;
use tokio::sync::Mutex;

// 匹配模式类型
#[derive(Debug, Clone)]
pub struct PatternType {
    pub host: PatternMatcher,
    pub path: Option<PatternMatcher>,
}

// 代理规则：匹配模式 -> 目标地址
#[derive(Debug, Clone)]
pub struct ProxyRule {
    pub pattern: AddressPattern,
    pub target: Address,
}

// 匹配结果：包含目标地址和匹配的路径前缀
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub target: Address,
    /// 匹配到的路径前缀（用于路径替换）
    /// 例如：pattern 是 "/console/api/*"，则 matched_path_prefix 是 "/console/api"
    pub matched_path_prefix: Option<String>,
}

// 精确匹配的索引键
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExactKey {
    protocol: Protocol,
    host: String,
    port: Option<u16>,
    path: Option<String>,
}

impl ExactKey {
    fn from_address(addr: &Address) -> Self {
        Self {
            protocol: addr.protocol,
            host: addr.host.clone(),
            port: addr.port,
            path: addr.path.clone(),
        }
    }
}

#[derive(Debug)]
// 代理管理器（优化版：混合索引 + LRU 缓存）
pub struct ProxyManager {
    // 精确匹配的快速索引 (O(1) 查找)
    exact_rules: HashMap<ExactKey, Address>,

    // 通配符和正则规则（需要遍历，但数量通常较少）
    pattern_rules: Vec<ProxyRule>,

    // LRU 缓存（缓存最近查询结果）- 使用 Mutex 实现内部可变性
    cache: Mutex<LruCache<String, Option<Address>>>,

    // 性能统计（原子）
    stats: ProxyStats,
}

/// 配置结构：使用 derive_builder 提供可配置的初始化
#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub struct ProxyManagerConfig {
    /// LRU 缓存大小
    #[builder(default = "1000")]
    pub cache_size: usize,

    /// 初始精确规则（可选）
    #[builder(default = "std::collections::HashMap::new()")]
    pub exact_rules: std::collections::HashMap<ExactKey, Address>,

    /// 初始模式规则（可选）
    #[builder(default = "Vec::new()")]
    pub pattern_rules: Vec<ProxyRule>,
}

impl ProxyManager {
    /// 使用 `ProxyManagerConfig` 构造
    pub fn from_config(cfg: ProxyManagerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let cache_size =
            NonZeroUsize::new(cfg.cache_size).ok_or_else(|| "cache_size must be non-zero")?;

        Ok(Self {
            exact_rules: cfg.exact_rules,
            pattern_rules: cfg.pattern_rules,
            cache: Mutex::new(LruCache::new(cache_size)),
            stats: ProxyStats::default(),
        })
    }

    /// 便捷访问 builder：`ProxyManagerConfig::builder()` 的包装
    pub fn builder() -> ProxyManagerConfigBuilder {
        ProxyManagerConfigBuilder::default()
    }
}

// 为 ProxyManager 添加可读的格式化输出
impl std::fmt::Display for ProxyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 基本统计信息
        let exact = self.exact_rule_count();
        let pattern = self.pattern_rule_count();

        writeln!(
            f,
            "ProxyManager Rules: total={} (exact={}, pattern={})",
            exact + pattern,
            exact,
            pattern
        )?;

        // 输出前 N 条规则（避免输出过多内容）
        const MAX_SHOW: usize = 20;
        let mut shown = 0;

        // 准备辅助格式化闭包
        let proto_str = |p: Protocol| match p {
            Protocol::Http => "http",
            Protocol::Https => "https",
            Protocol::Ws => "ws",
            Protocol::Wss => "wss",
        };

        let fmt_address = |a: &Address| {
            let scheme = proto_str(a.protocol);
            let authority = if let Some(port) = a.port {
                format!("{}:{}", a.host, port)
            } else {
                a.host.clone()
            };
            let path = a.path.as_deref().unwrap_or("/");
            format!("{}://{}{}", scheme, authority, path)
        };

        let fmt_pattern = |p: &AddressPattern| {
            let scheme = proto_str(p.protocol);
            let host_pat = match &p.pattern_type.host {
                PatternMatcher::Exact(s) => s.clone(),
                PatternMatcher::Wildcard(s) => s.clone(),
                PatternMatcher::Regex { pattern, .. } => format!("re:{}", pattern),
            };
            let port = if let Some(port) = p.port {
                format!(":{}", port)
            } else {
                String::new()
            };
            let path_pat = match &p.pattern_type.path {
                None => String::new(),
                Some(PatternMatcher::Exact(s)) => s.clone(),
                Some(PatternMatcher::Wildcard(s)) => s.clone(),
                Some(PatternMatcher::Regex { pattern, .. }) => format!("re:{}", pattern),
            };
            format!("{}://{}{}{}", scheme, host_pat, port, path_pat)
        };

        // 输出精确规则（每条一行）
        for (key, target) in &self.exact_rules {
            if shown >= MAX_SHOW {
                break;
            }
            let path = key.path.as_deref().unwrap_or("/");
            let pat = format!("{}://{}{}", proto_str(key.protocol), key.host, path);
            writeln!(f, "EXACT {} -> {}", pat, fmt_address(target))?;
            shown += 1;
        }

        // 如果还没到上限，继续输出 pattern rules
        if shown < MAX_SHOW {
            for rule in &self.pattern_rules {
                if shown >= MAX_SHOW {
                    break;
                }
                // 简单序列化 pattern -> target（单行）
                writeln!(
                    f,
                    "PATTERN {} -> {}",
                    fmt_pattern(&rule.pattern),
                    fmt_address(&rule.target)
                )?;
                shown += 1;
            }
        }

        if exact + pattern > MAX_SHOW {
            writeln!(
                f,
                "...and {} more rules omitted",
                exact + pattern - MAX_SHOW
            )?;
        }

        Ok(())
    }
}

impl ProxyManager {
    /// 添加代理规则
    ///
    /// 规则会自动分类到精确索引或模式列表中以优化查找性能
    pub async fn add_rule(&mut self, pattern: AddressPattern, target: Address) {
        // 检查是否为精确匹配（可以使用快速索引）
        let is_exact = matches!(&pattern.pattern_type.host, PatternMatcher::Exact(_))
            && pattern
                .pattern_type
                .path
                .as_ref()
                .is_none_or(|p| matches!(p, PatternMatcher::Exact(_)));

        if is_exact {
            // 提取精确匹配的键
            if let PatternMatcher::Exact(host) = &pattern.pattern_type.host {
                let path = pattern.pattern_type.path.as_ref().and_then(|p| {
                    if let PatternMatcher::Exact(path_str) = p {
                        Some(path_str.clone())
                    } else {
                        None
                    }
                });

                let key = ExactKey {
                    protocol: pattern.protocol,
                    host: host.clone(),
                    port: pattern.port,
                    path,
                };

                self.exact_rules.insert(key, target);
                return;
            }
        }

        // 非精确匹配，添加到模式列表
        self.pattern_rules.push(ProxyRule { pattern, target });

        // 清空缓存（规则变化）
        self.cache.lock().await.clear();
    }

    /// 从 Uri 查找匹配的目标地址（带缓存）
    pub async fn find_target(&self, uri: &Uri) -> Option<Address> {
        // 记录总查询（原子，低开销）
        self.stats.inc_total();

        let uri_str = uri.to_string();

        // 1. 检查缓存
        {
            let mut cache = self.cache.lock().await;
            if let Some(cached) = cache.get(&uri_str) {
                self.stats.inc_cache();
                return cached.clone();
            }
        }

        // 2. 解析 Uri 为 Address
        let address = Address::from_uri(uri).ok()?;
        let result = self.find_target_for_address_uncached(&address).await;

        // 3. 更新缓存
        let mut cache = self.cache.lock().await;
        cache.put(uri_str, result.clone());

        result
    }

    /// 从 Uri 查找匹配的目标地址，返回匹配详情（包含路径前缀信息）
    ///
    /// 返回 `MatchResult` 包含：
    /// - `target`: 目标地址
    /// - `matched_path_prefix`: 匹配到的路径前缀（用于路径替换）
    pub async fn find_target_with_match_info(&self, uri: &Uri) -> Option<MatchResult> {
        self.stats.inc_total();

        // 解析 Uri 为 Address
        let address = Address::from_uri(uri).ok()?;

        // 1. 先查精确索引 (O(1))
        let key = ExactKey::from_address(&address);
        if let Some(target) = self.exact_rules.get(&key) {
            self.stats.inc_exact();
            return Some(MatchResult {
                target: target.clone(),
                matched_path_prefix: key.path.clone(),
            });
        }

        // 2. 遍历模式规则 (O(n)，但 n 通常很小)
        for rule in &self.pattern_rules {
            if rule.pattern.matches(&address) {
                self.stats.inc_pattern();

                // 提取匹配的路径前缀
                let matched_path_prefix =
                    if let Some(path_pattern) = &rule.pattern.pattern_type.path {
                        match path_pattern {
                            PatternMatcher::Exact(p) => Some(p.clone()),
                            PatternMatcher::Wildcard(p) => {
                                // 去掉通配符 * 得到前缀
                                Some(p.trim_end_matches('*').to_string())
                            }
                            PatternMatcher::Regex { .. } => {
                                // 正则模式暂不支持路径替换，返回 None
                                None
                            }
                        }
                    } else {
                        None
                    };

                return Some(MatchResult {
                    target: rule.target.clone(),
                    matched_path_prefix,
                });
            }
        }

        self.stats.inc_miss();
        None
    }

    /// 内部查找方法（更新统计）
    async fn find_target_for_address_uncached(&self, address: &Address) -> Option<Address> {
        // 1. 先查精确索引 (O(1))
        let key = ExactKey::from_address(address);
        if let Some(target) = self.exact_rules.get(&key) {
            self.stats.inc_exact();
            return Some(target.clone());
        }

        // 2. 遍历模式规则 (O(n)，但 n 通常很小)
        for rule in &self.pattern_rules {
            if rule.pattern.matches(address) {
                self.stats.inc_pattern();
                return Some(rule.target.clone());
            }
        }
        self.stats.inc_miss();
        None
    }

    /// 获取所有规则（包括精确和模式规则）
    pub fn all_rules(&self) -> Vec<ProxyRule> {
        let mut rules = Vec::new();

        // 添加精确规则
        for (key, target) in &self.exact_rules {
            let pattern = AddressPattern {
                protocol: key.protocol,
                port: key.port,
                pattern_type: PatternType {
                    host: PatternMatcher::Exact(key.host.clone()),
                    path: key.path.as_ref().map(|p| PatternMatcher::Exact(p.clone())),
                },
            };
            rules.push(ProxyRule {
                pattern,
                target: target.clone(),
            });
        }

        // 添加模式规则
        rules.extend(self.pattern_rules.clone());

        rules
    }

    /// 获取模式规则（仅通配符和正则）
    pub fn pattern_rules(&self) -> &[ProxyRule] {
        &self.pattern_rules
    }

    /// 获取精确规则数量
    pub fn exact_rule_count(&self) -> usize {
        self.exact_rules.len()
    }

    /// 获取模式规则数量
    pub fn pattern_rule_count(&self) -> usize {
        self.pattern_rules.len()
    }

    /// 获取性能统计（快照）
    pub async fn stats(&self) -> ProxyStatsSnapshot {
        // 读取原子快照
        self.stats.snapshot()
    }

    /// 重置性能统计
    pub async fn reset_stats(&self) {
        self.stats.reset();
    }

    /// 清空所有规则和缓存
    pub async fn clear(&mut self) {
        self.exact_rules.clear();
        self.pattern_rules.clear();
        self.cache.lock().await.clear();
        self.stats.reset();
    }

    /// 清空缓存（保留规则）
    pub async fn clear_cache(&self) {
        self.cache.lock().await.clear();
    }
}
