use regex::Regex;

// 单个字段的模式匹配器
#[derive(Debug, Clone)]
pub enum PatternMatcher {
    /// 精确匹配
    Exact(String),
    /// 前缀或后缀匹配（通配符 '*'）
    Wildcard(String), // 保留原始模式字符串（含 *）
    /// 正则表达式匹配
    Regex { compiled: Regex, pattern: String },
}

impl PatternMatcher {
    pub(crate) fn from_str(s: &str) -> Result<Self, regex::Error> {
        if let Some(rest) = s.strip_prefix("re:") {
            Ok(PatternMatcher::Regex {
                compiled: Regex::new(rest)?,
                pattern: s.to_string(),
            })
        } else if s.contains('*') {
            Ok(PatternMatcher::Wildcard(s.to_string()))
        } else {
            Ok(PatternMatcher::Exact(s.to_string()))
        }
    }

    pub(crate) fn matches(&self, value: &str) -> bool {
        match self {
            PatternMatcher::Exact(pattern) => value == pattern,
            PatternMatcher::Wildcard(pattern) => {
                if let Some(suffix) = pattern.strip_prefix('*') {
                    // 后缀匹配，如 *.example.com
                    value.ends_with(suffix)
                } else if let Some(prefix) = pattern.strip_suffix('*') {
                    // 前缀匹配，如 example.*
                    value.starts_with(prefix)
                } else {
                    // 中间包含 *，暂不支持复杂模式，回退到精确匹配
                    value == pattern
                }
            }
            PatternMatcher::Regex { compiled, .. } => compiled.is_match(value),
        }
    }
}

impl std::fmt::Display for PatternMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternMatcher::Exact(pattern) | PatternMatcher::Wildcard(pattern) => {
                write!(f, "{}", pattern)
            }
            PatternMatcher::Regex { pattern, .. } => {
                if pattern.starts_with("re:") {
                    write!(f, "{}", pattern)
                } else {
                    write!(f, "re:{}", pattern)
                }
            }
        }
    }
}
