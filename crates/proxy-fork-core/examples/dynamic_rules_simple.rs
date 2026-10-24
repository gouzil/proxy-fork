// 简单的动态规则添加示例
// 演示如何在运行时动态添加代理规则

use proxy_fork_core::{
    PathTransformMode,
    http_address::{Address, AddressPattern, Protocol},
    proxy_manage::ProxyManager,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() {
    // 使用简单的 println 输出（生产环境建议使用 tracing）

    // 创建共享的 ProxyManager
    let proxy_manager = Arc::new(RwLock::new(
        ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
            .expect("Failed to construct ProxyManager from config"),
    ));

    // 添加初始规则
    {
        let mut manager = proxy_manager.write().await;

        let pattern = AddressPattern::new(Protocol::Http, "example.com", None, Some("/api/*"))
            .expect("Failed to create pattern");

        let target = Address {
            protocol: Protocol::Http,
            host: "localhost".to_string(),
            port: Some(3000),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };

        manager.add_rule(pattern, target).await;
        println!("✅ 添加初始规则，总数: {}", manager.all_rules().len());
    }

    // 模拟：代理服务器在运行
    let manager_clone = proxy_manager.clone();
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(2)).await;

            // 模拟处理请求（使用只读锁）
            let manager = manager_clone.read().await;
            let test_uri: http::Uri = "http://example.com/api/users".parse().unwrap();

            if let Some(target) = manager.find_target(&test_uri).await {
                println!(
                    "🔄 请求路由: {} -> {}:{}",
                    test_uri,
                    target.host,
                    target.port.unwrap_or(80)
                );
            }

            // 显示统计
            let stats = manager.stats().await;
            println!(
                "📊 统计 - 总查询: {}, 缓存命中: {}, 精确命中: {}, 模式命中: {}, 未命中: {}",
                stats.total_lookups,
                stats.cache_hits,
                stats.exact_hits,
                stats.pattern_hits,
                stats.misses
            );
        }
    });

    // 模拟：用户通过某种方式动态添加规则
    tokio::spawn(async move {
        sleep(Duration::from_secs(5)).await;

        // 5秒后添加新规则
        {
            let mut manager = proxy_manager.write().await;

            println!("🎯 正在添加新规则...");

            let pattern = AddressPattern::new(Protocol::Http, "*.test.com", None, None)
                .expect("Failed to create pattern");

            let target = Address {
                protocol: Protocol::Http,
                host: "test-backend".to_string(),
                port: Some(4000),
                path: None,
                path_transform_mode: PathTransformMode::default(),
            };

            manager.add_rule(pattern, target).await;
            println!("✨ 动态添加规则成功！总数: {}", manager.all_rules().len());
        }

        sleep(Duration::from_secs(5)).await;

        // 10秒后再添加一个正则规则
        {
            let mut manager = proxy_manager.write().await;

            println!("🎯 正在添加正则规则...");

            let pattern = AddressPattern::new(
                Protocol::Http,
                "re:^dev\\d+\\.example\\.com$",
                None,
                Some("/api/v[0-9]+/*"),
            )
            .expect("Failed to create pattern");

            let target = Address {
                protocol: Protocol::Http,
                host: "dev-cluster".to_string(),
                port: Some(5000),
                path: None,
                path_transform_mode: PathTransformMode::default(),
            };

            manager.add_rule(pattern, target).await;
            println!("✨ 正则规则添加成功！总数: {}", manager.all_rules().len());
        }

        sleep(Duration::from_secs(5)).await;

        // 15秒后查看所有规则
        {
            let manager = proxy_manager.read().await;
            let all_rules = manager.all_rules();

            println!("📋 当前所有规则 ({} 条):", all_rules.len());
            for (i, rule) in all_rules.iter().enumerate() {
                println!(
                    "  [{}] {:?}://{:?}:{}{} -> {}:{}",
                    i + 1,
                    rule.pattern.protocol,
                    rule.pattern.pattern_type.host,
                    rule.pattern.port.map_or("*".to_string(), |p| p.to_string()),
                    rule.pattern
                        .pattern_type
                        .path
                        .as_ref()
                        .map_or("/*".to_string(), |p| format!("{:?}", p)),
                    rule.target.host,
                    rule.target.port.unwrap_or(80)
                );
            }

            println!("📊 规则类型统计:");
            println!("  - 精确规则: {}", manager.exact_rule_count());
            println!("  - 模式规则: {}", manager.pattern_rule_count());
        }
    });

    // 主线程等待
    println!("🚀 ProxyManager 动态规则示例运行中...");
    println!("💡 将在运行期间动态添加规则，观察日志输出");

    sleep(Duration::from_secs(20)).await;

    println!("✅ 示例运行完成");
}
