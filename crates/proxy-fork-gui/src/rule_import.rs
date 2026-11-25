use anyhow::{Context, Result, ensure};
use proxy_fork_core::app_config::{FileConfig, ProxyManagerSection, RuleItem};
use std::{io::Read, path::PathBuf, time::Duration};

const MAX_IMPORT_BYTES: usize = 1024 * 1024;

pub fn parse_rules(bytes: &[u8]) -> Result<Vec<RuleItem>> {
    ensure!(bytes.len() <= MAX_IMPORT_BYTES, "rule import exceeds 1 MiB");
    let text = std::str::from_utf8(bytes)
        .context("rules must be UTF-8 text")?
        .trim_start_matches('\u{feff}');
    let config: FileConfig = toml::from_str(text).context("invalid TOML rules file")?;
    let rules = match config.proxy_manager.and_then(|manager| manager.rules) {
        Some(rules) => rules,
        None => toml::from_str::<ProxyManagerSection>(text)?
            .rules
            .context("expected proxy_manager.rules or rules in the TOML file")?,
    };
    ensure!(!rules.is_empty(), "the import contains no rules");
    for (index, rule) in rules.iter().enumerate() {
        rule.runtime()
            .with_context(|| format!("invalid imported rule {}", index + 1))?;
    }
    Ok(rules)
}

pub async fn load_files(files: Vec<PathBuf>) -> Result<Vec<RuleItem>> {
    ensure!(!files.is_empty(), "select at least one rules file");
    tokio::task::spawn_blocking(move || {
        let mut rules = Vec::new();
        let mut size = 0;
        for path in files {
            ensure!(fs_err::metadata(&path)?.is_file(), "expected a rules file");
            let mut bytes = Vec::new();
            fs_err::File::open(&path)?
                .take((MAX_IMPORT_BYTES - size + 1) as u64)
                .read_to_end(&mut bytes)?;
            size += bytes.len();
            ensure!(size <= MAX_IMPORT_BYTES, "rule import exceeds 1 MiB");
            rules.extend(
                parse_rules(&bytes).with_context(|| format!("cannot import {}", path.display()))?,
            );
        }
        Ok(rules)
    })
    .await?
}

pub async fn load_source(source: &str) -> Result<Vec<RuleItem>> {
    let source = source.trim();
    ensure!(!source.is_empty(), "enter a rules file path or HTTP(S) URL");
    if !source.contains("://") {
        return load_files(vec![source.into()]).await;
    }
    let url = reqwest::Url::parse(source).context("invalid rules URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "rules URL must use HTTP or HTTPS"
    );
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()?;
    let mut response = client.get(url).send().await?.error_for_status()?;
    ensure!(
        response.content_length().unwrap_or(0) <= MAX_IMPORT_BYTES as u64,
        "rule import exceeds 1 MiB"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            chunk.len() <= MAX_IMPORT_BYTES - bytes.len(),
            "rule import exceeds 1 MiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    tokio::task::spawn_blocking(move || parse_rules(&bytes)).await?
}
