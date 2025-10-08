use codspeed_criterion_compat::{BenchmarkId, Criterion, criterion_group, criterion_main};
use http::Uri;
use proxy_fork_core::{
    PathTransformMode,
    http_address::{Address, AddressPattern, Protocol},
    proxy_manage::ProxyManager,
};
use tokio::runtime::Runtime;

/// 创建测试用的 ProxyManager，包含多条规则
fn create_manager_with_rules(exact_count: usize, pattern_count: usize) -> ProxyManager {
    let rt = Runtime::new().unwrap();
    let mut manager =
        ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
            .expect("Failed to construct ProxyManager from config");

    // 添加精确匹配规则
    for i in 0..exact_count {
        let pattern = AddressPattern::new(
            Protocol::Http,
            &format!("exact{}.example.com", i),
            Some(80),
            Some(&format!("/api/v{}", i)),
        )
        .unwrap();

        let target = Address {
            protocol: Protocol::Http,
            host: format!("backend{}", i),
            port: Some(3000 + i as u16),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };

        rt.block_on(async {
            manager.add_rule(pattern, target).await;
        });
    }

    // 添加模式匹配规则
    for i in 0..pattern_count {
        let pattern =
            AddressPattern::new(Protocol::Http, &format!("*.domain{}.com", i), None, None).unwrap();

        let target = Address {
            protocol: Protocol::Http,
            host: format!("wildcard-backend{}", i),
            port: Some(4000 + i as u16),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };

        rt.block_on(async {
            manager.add_rule(pattern, target).await;
        });
    }

    manager
}

/// 基准测试：精确匹配查找（O(1) HashMap）
fn bench_exact_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("exact_match");
    let rt = Runtime::new().unwrap();

    for rule_count in [10, 50, 100, 500, 1000].iter() {
        let manager = create_manager_with_rules(*rule_count, 0);
        let uri: Uri = format!(
            "http://exact{}.example.com:80/api/v{}",
            rule_count / 2,
            rule_count / 2
        )
        .parse()
        .unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_rules", rule_count)),
            rule_count,
            |b, _| {
                b.iter(|| {
                    rt.block_on(async { manager.find_target(&uri).await });
                });
            },
        );
    }

    group.finish();
}

/// 基准测试：模式匹配查找（O(n) Vec）
fn bench_pattern_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("pattern_match");
    let rt = Runtime::new().unwrap();

    for pattern_count in [5, 10, 20, 50, 100].iter() {
        let manager = create_manager_with_rules(0, *pattern_count);
        let uri: Uri = format!("http://api.domain{}.com/test", pattern_count / 2)
            .parse()
            .unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_patterns", pattern_count)),
            pattern_count,
            |b, _| {
                b.iter(|| {
                    rt.block_on(async { manager.find_target(&uri).await });
                });
            },
        );
    }

    group.finish();
}

/// 基准测试：缓存命中（O(1) LRU）
fn bench_cache_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_hit");
    let rt = Runtime::new().unwrap();

    let manager = create_manager_with_rules(50, 20);
    let uri: Uri = "http://exact25.example.com:80/api/v25".parse().unwrap();

    // 预热缓存
    rt.block_on(async { manager.find_target(&uri).await });

    group.bench_function("cached_lookup", |b| {
        b.iter(|| {
            rt.block_on(async { manager.find_target(&uri).await });
        });
    });

    group.finish();
}

/// 基准测试：混合场景
fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");
    let rt = Runtime::new().unwrap();

    let manager = create_manager_with_rules(100, 20);

    let test_uris = vec![
        "http://exact50.example.com:80/api/v50"
            .parse::<Uri>()
            .unwrap(), // 精确匹配
        "http://api.domain10.com/test".parse().unwrap(), // 模式匹配
        "http://exact50.example.com:80/api/v50".parse().unwrap(), // 缓存命中
        "http://unknown.com/test".parse().unwrap(),      // 未匹配
    ];

    group.bench_function("realistic_workload", |b| {
        b.iter(|| {
            rt.block_on(async {
                for uri in &test_uris {
                    manager.find_target(uri).await;
                }
            });
        });
    });

    group.finish();
}

/// 基准测试：规则添加
fn bench_add_rule(c: &mut Criterion) {
    let mut group = c.benchmark_group("add_rule");
    let rt = Runtime::new().unwrap();

    group.bench_function("add_exact_rule", |b| {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");
        let mut counter = 0;

        b.iter(|| {
            let pattern = AddressPattern::new(
                Protocol::Http,
                &format!("exact{}.example.com", counter),
                Some(80),
                Some(&format!("/api/v{}", counter)),
            )
            .unwrap();

            let target = Address {
                protocol: Protocol::Http,
                host: format!("backend{}", counter),
                port: Some(3000),
                path: None,
                path_transform_mode: PathTransformMode::default(),
            };

            rt.block_on(async {
                manager.add_rule(pattern, target).await;
            });
            counter += 1;
        });
    });

    group.bench_function("add_pattern_rule", |b| {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");
        let mut counter = 0;

        b.iter(|| {
            let pattern = AddressPattern::new(
                Protocol::Http,
                &format!("*.domain{}.com", counter),
                None,
                None,
            )
            .unwrap();

            let target = Address {
                protocol: Protocol::Http,
                host: format!("backend{}", counter),
                port: Some(3000),
                path: None,
                path_transform_mode: PathTransformMode::default(),
            };

            rt.block_on(async {
                manager.add_rule(pattern, target).await;
            });
            counter += 1;
        });
    });

    group.finish();
}

/// 基准测试：大规模规则集
fn bench_large_ruleset(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_ruleset");
    group.sample_size(50); // 大规模测试减少样本数
    let rt = Runtime::new().unwrap();

    for (exact, pattern) in [(500, 100), (1000, 200), (2000, 500)].iter() {
        let manager = create_manager_with_rules(*exact, *pattern);
        let uri: Uri = format!(
            "http://exact{}.example.com:80/api/v{}",
            exact / 2,
            exact / 2
        )
        .parse()
        .unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}exact_{}pattern", exact, pattern)),
            &(exact, pattern),
            |b, _| {
                b.iter(|| {
                    rt.block_on(async { manager.find_target(&uri).await });
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_exact_match,
    bench_pattern_match,
    bench_cache_hit,
    bench_mixed_workload,
    bench_add_rule,
    bench_large_ruleset,
);

criterion_main!(benches);
