#[cfg(test)]
mod proxy_manager_test {
    use http::Uri;
    use proxy_fork_core::{
        PathTransformMode,
        http_address::{Address, AddressPattern, Protocol},
        proxy_manage::ProxyManager,
    };

    #[tokio::test]
    async fn test_proxy_manager_basic() {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 添加规则：将 api.example.com/console/api/* 代理到 localhost:5001
        let pattern = AddressPattern::new(
            Protocol::Http,
            "api.example.com",
            None,
            Some("/console/api/*"),
        )
        .unwrap();

        let target = Address {
            protocol: Protocol::Http,
            host: "localhost".to_string(),
            port: Some(5001),
            path: Some("/console/api/".to_string()),
            path_transform_mode: PathTransformMode::default(),
        };

        manager.add_rule(pattern, target.clone()).await;

        // 测试匹配
        let test_uri: Uri = "http://api.example.com/console/api/users".parse().unwrap();
        let result = manager.find_target(&test_uri).await;

        assert!(result.is_some());
        let found = result.unwrap();
        assert_eq!(found.host, "localhost");
        assert_eq!(found.port, Some(5001));

        // 测试不匹配的路径
        let test_uri2: Uri = "http://api.example.com/other/path".parse().unwrap();
        let result2 = manager.find_target(&test_uri2).await;
        assert!(result2.is_none());
    }

    #[tokio::test]
    async fn test_proxy_manager_multiple_rules() {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 规则 1: *.api.com -> api-gateway:8080
        let pattern1 = AddressPattern::new(Protocol::Https, "*.api.com", None, None).unwrap();
        let target1 = Address {
            protocol: Protocol::Http,
            host: "api-gateway".to_string(),
            port: Some(8080),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern1, target1).await;

        // 规则 2: example.com/api/* -> backend:3000
        let pattern2 =
            AddressPattern::new(Protocol::Http, "example.com", None, Some("/api/*")).unwrap();
        let target2 = Address {
            protocol: Protocol::Http,
            host: "backend".to_string(),
            port: Some(3000),
            path: Some("/api/".to_string()),
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern2, target2).await;

        // 测试第一个规则
        let uri1: Uri = "https://prod.api.com/users".parse().unwrap();
        let result1 = manager.find_target(&uri1).await;
        assert!(result1.is_some());
        assert_eq!(result1.unwrap().host, "api-gateway");

        // 测试第二个规则
        let uri2: Uri = "http://example.com/api/posts".parse().unwrap();
        let result2 = manager.find_target(&uri2).await;
        assert!(result2.is_some());
        assert_eq!(result2.unwrap().host, "backend");

        // 测试不匹配任何规则
        let uri3: Uri = "http://other.com/path".parse().unwrap();
        let result3 = manager.find_target(&uri3).await;
        assert!(result3.is_none());
    }

    #[tokio::test]
    async fn test_proxy_manager_rule_priority() {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 添加两个可能冲突的规则（第一个匹配优先）
        let pattern1 =
            AddressPattern::new(Protocol::Http, "example.com", None, Some("/api/v1/*")).unwrap();
        let target1 = Address {
            protocol: Protocol::Http,
            host: "backend-v1".to_string(),
            port: Some(3001),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern1, target1).await;

        let pattern2 =
            AddressPattern::new(Protocol::Http, "example.com", None, Some("/api/*")).unwrap();
        let target2 = Address {
            protocol: Protocol::Http,
            host: "backend-general".to_string(),
            port: Some(3000),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern2, target2).await;

        // 应该匹配第一个更具体的规则
        let uri: Uri = "http://example.com/api/v1/users".parse().unwrap();
        let result = manager.find_target(&uri).await;

        assert!(result.is_some());
        assert_eq!(result.unwrap().host, "backend-v1");
    }

    #[tokio::test]
    async fn test_proxy_manager_with_regex() {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 使用正则匹配多个子域名
        let pattern = AddressPattern::new(
            Protocol::Https,
            "re:^(prod|test|dev)\\.api\\.com$",
            None,
            None,
        )
        .unwrap();

        let target = Address {
            protocol: Protocol::Http,
            host: "internal-api".to_string(),
            port: Some(8080),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern, target).await;

        // 测试匹配
        let uri1: Uri = "https://prod.api.com/users".parse().unwrap();
        assert!(manager.find_target(&uri1).await.is_some());

        let uri2: Uri = "https://test.api.com/users".parse().unwrap();
        assert!(manager.find_target(&uri2).await.is_some());

        let uri3: Uri = "https://dev.api.com/users".parse().unwrap();
        assert!(manager.find_target(&uri3).await.is_some());

        // 不匹配
        let uri4: Uri = "https://staging.api.com/users".parse().unwrap();
        assert!(manager.find_target(&uri4).await.is_none());
    }

    #[tokio::test]
    async fn test_proxy_manager_clear() {
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        let pattern = AddressPattern::new(Protocol::Http, "example.com", None, None).unwrap();
        let target = Address {
            protocol: Protocol::Http,
            host: "backend".to_string(),
            port: Some(3000),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern, target).await;

        assert_eq!(manager.all_rules().len(), 1);

        manager.clear().await;
        assert_eq!(manager.all_rules().len(), 0);

        // 清空后不应匹配任何规则
        let uri: Uri = "http://example.com/test".parse().unwrap();
        assert!(manager.find_target(&uri).await.is_none());
    }

    #[tokio::test]
    async fn test_proxy_manager_exact_vs_pattern() {
        // 测试精确匹配和模式匹配的区分
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 添加精确匹配规则
        let exact_pattern =
            AddressPattern::new(Protocol::Http, "exact.example.com", Some(80), Some("/api"))
                .unwrap();
        let exact_target = Address {
            protocol: Protocol::Http,
            host: "exact-backend".to_string(),
            port: Some(3001),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(exact_pattern, exact_target).await;

        // 添加通配符规则（模式匹配）
        let wildcard_pattern =
            AddressPattern::new(Protocol::Http, "*.example.com", None, None).unwrap();
        let wildcard_target = Address {
            protocol: Protocol::Http,
            host: "wildcard-backend".to_string(),
            port: Some(3002),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(wildcard_pattern, wildcard_target).await;

        // 验证精确规则被放入快速索引
        assert_eq!(manager.exact_rule_count(), 1);
        assert_eq!(manager.pattern_rule_count(), 1);

        // 测试精确匹配（O(1) 查找）
        let exact_uri: Uri = "http://exact.example.com:80/api".parse().unwrap();
        let result = manager.find_target(&exact_uri).await.unwrap();
        assert_eq!(result.host, "exact-backend");

        // 测试通配符匹配（O(n) 查找）
        let wildcard_uri: Uri = "http://api.example.com/test".parse().unwrap();
        let result = manager.find_target(&wildcard_uri).await.unwrap();
        assert_eq!(result.host, "wildcard-backend");
    }

    #[tokio::test]
    async fn test_proxy_manager_cache() {
        // 测试 LRU 缓存功能
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        let pattern = AddressPattern::new(Protocol::Http, "example.com", None, None).unwrap();
        let target = Address {
            protocol: Protocol::Http,
            host: "backend".to_string(),
            port: Some(3000),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern, target).await;

        let uri: Uri = "http://example.com/test".parse().unwrap();

        // 第一次查询（缓存未命中）
        manager.find_target(&uri).await;
        let stats1 = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats1.total_lookups, 1);
            assert_eq!(stats1.cache_hits, 0);
        }

        // 第二次查询相同 URI（缓存命中）
        manager.find_target(&uri).await;
        let stats2 = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats2.total_lookups, 2);
            assert_eq!(stats2.cache_hits, 1);
        }

        // 再次查询（缓存命中）
        manager.find_target(&uri).await;
        let stats3 = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats3.total_lookups, 3);
            assert_eq!(stats3.cache_hits, 2);
        }
    }

    #[tokio::test]
    async fn test_proxy_manager_stats() {
        // 测试性能统计
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 添加精确匹配规则（包含完整路径）
        let exact_pattern =
            AddressPattern::new(Protocol::Http, "exact.example.com", Some(80), Some("/test"))
                .unwrap();
        let exact_target = Address {
            protocol: Protocol::Http,
            host: "backend1".to_string(),
            port: Some(3001),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(exact_pattern, exact_target).await;

        // 添加模式匹配规则
        let pattern = AddressPattern::new(Protocol::Http, "*.example.com", None, None).unwrap();
        let target = Address {
            protocol: Protocol::Http,
            host: "backend2".to_string(),
            port: Some(3002),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern, target).await;

        // 执行多次查询
        let exact_uri: Uri = "http://exact.example.com:80/test".parse().unwrap();
        manager.find_target(&exact_uri).await; // exact hit
        manager.find_target(&exact_uri).await; // cache hit

        let pattern_uri: Uri = "http://api.example.com/test".parse().unwrap();
        manager.find_target(&pattern_uri).await; // pattern hit
        manager.find_target(&pattern_uri).await; // cache hit

        let miss_uri: Uri = "http://other.com/test".parse().unwrap();
        manager.find_target(&miss_uri).await; // miss

        let stats = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats.total_lookups, 5);
            assert_eq!(stats.cache_hits, 2);
            assert_eq!(stats.exact_hits, 1);
            assert_eq!(stats.pattern_hits, 1);
            assert_eq!(stats.misses, 1);

            // 测试命中率计算
            assert_eq!(stats.hit_rate(), 0.8); // 4/5
            assert_eq!(stats.cache_hit_rate(), 0.4); // 2/5

            // 重置统计
            manager.reset_stats().await;
            let reset_stats = manager.stats().await;
            assert_eq!(reset_stats.total_lookups, 0);
        }
    }

    #[tokio::test]
    async fn test_proxy_manager_cache_invalidation() {
        // 测试添加规则后缓存自动失效
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        let pattern1 =
            AddressPattern::new(Protocol::Http, "example.com", None, Some("/api")).unwrap();
        let target1 = Address {
            protocol: Protocol::Http,
            host: "backend1".to_string(),
            port: Some(3001),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern1, target1).await;

        let uri: Uri = "http://example.com/api".parse().unwrap();

        // 第一次查询并缓存
        let result1 = manager.find_target(&uri).await.unwrap();
        assert_eq!(result1.host, "backend1");

        // 验证已经缓存
        let stats1 = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats1.cache_hits, 0); // 第一次查询不命中缓存
        }

        // 再次查询应该命中缓存
        let cached_result = manager.find_target(&uri).await.unwrap();
        assert_eq!(cached_result.host, "backend1");
        let stats2 = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats2.cache_hits, 1);
        }

        // 清空所有规则并添加新规则（缓存应该被清空）
        manager.clear().await;

        let pattern2 =
            AddressPattern::new(Protocol::Http, "example.com", None, Some("/api")).unwrap();
        let target2 = Address {
            protocol: Protocol::Http,
            host: "backend2".to_string(),
            port: Some(3002),
            path: None,
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern2, target2).await;

        // 再次查询（应该匹配新规则，缓存被清空了）
        let result2 = manager.find_target(&uri).await.unwrap();
        assert_eq!(result2.host, "backend2");

        // 验证统计已重置（clear 会重置统计）
        let stats3 = manager.stats().await;
        if cfg!(feature = "proxy_manage_stats") {
            assert_eq!(stats3.total_lookups, 1); // clear 后只有 1 次查询
            assert_eq!(stats3.cache_hits, 0); // 没有缓存命中
        }
    }

    #[tokio::test]
    async fn test_proxy_manager_real_world_scenario() {
        // 模拟真实场景：本地开发代理配置
        let mut manager =
            ProxyManager::from_config(ProxyManager::builder().cache_size(1000).build().unwrap())
                .expect("Failed to construct ProxyManager from config");

        // 1. 开发环境 API 代理到本地后端
        let pattern1 = AddressPattern::new(
            Protocol::Http,
            "api.example.com",
            None,
            Some("/console/api/*"),
        )
        .unwrap();
        let target1 = Address {
            protocol: Protocol::Http,
            host: "localhost".to_string(),
            port: Some(5001),
            path: Some("/console/api/".to_string()),
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern1, target1).await;

        // 2. 静态资源保持原样（通过不添加规则实现）

        // 3. WebSocket 连接代理
        let pattern3 =
            AddressPattern::new(Protocol::Http, "api.example.com", None, Some("/ws/*")).unwrap();
        let target3 = Address {
            protocol: Protocol::Http,
            host: "localhost".to_string(),
            port: Some(5002),
            path: Some("/ws/".to_string()),
            path_transform_mode: PathTransformMode::default(),
        };
        manager.add_rule(pattern3, target3).await;

        // 测试 API 代理
        let api_uri: Uri = "http://api.example.com/console/api/users".parse().unwrap();
        let api_result = manager.find_target(&api_uri).await.unwrap();
        assert_eq!(api_result.host, "localhost");
        assert_eq!(api_result.port, Some(5001));

        // 测试 WebSocket 代理
        let ws_uri: Uri = "http://api.example.com/ws/chat".parse().unwrap();
        let ws_result = manager.find_target(&ws_uri).await.unwrap();
        assert_eq!(ws_result.host, "localhost");
        assert_eq!(ws_result.port, Some(5002));

        // 测试静态资源（不匹配任何规则）
        let static_uri: Uri = "http://api.example.com/static/app.js".parse().unwrap();
        let static_result = manager.find_target(&static_uri).await;
        assert!(static_result.is_none());
    }
}
