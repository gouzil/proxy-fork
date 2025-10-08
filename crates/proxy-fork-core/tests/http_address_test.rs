#[cfg(test)]
mod address_pattern_test {
    use http::Uri;
    use proxy_fork_core::{
        PathTransformMode,
        http_address::{Address, AddressPattern, Protocol},
    };

    fn create_address(
        protocol: Protocol,
        host: &str,
        port: Option<u16>,
        path: Option<&str>,
    ) -> Address {
        Address {
            protocol,
            host: host.to_string(),
            port,
            path: path.map(|s| s.to_string()),
            path_transform_mode: PathTransformMode::default(),
        }
    }

    #[test]
    fn test_exact_match() {
        let pattern =
            AddressPattern::new(Protocol::Http, "example.com", Some(80), Some("/api")).unwrap();

        // 完全匹配
        let addr1 = create_address(Protocol::Http, "example.com", Some(80), Some("/api"));
        assert!(pattern.matches(&addr1));

        // host 不匹配
        let addr2 = create_address(Protocol::Http, "other.com", Some(80), Some("/api"));
        assert!(!pattern.matches(&addr2));

        // path 不匹配
        let addr3 = create_address(Protocol::Http, "example.com", Some(80), Some("/api/v2"));
        assert!(!pattern.matches(&addr3));

        // protocol 不匹配
        let addr4 = create_address(Protocol::Https, "example.com", Some(80), Some("/api"));
        assert!(!pattern.matches(&addr4));
    }

    #[test]
    fn test_prefix_match() {
        let pattern =
            AddressPattern::new(Protocol::Http, "example.com", None, Some("/api/*")).unwrap();

        // path 前缀匹配
        let addr1 = create_address(Protocol::Http, "example.com", Some(80), Some("/api/v1"));
        assert!(pattern.matches(&addr1));

        let addr2 = create_address(
            Protocol::Http,
            "example.com",
            Some(80),
            Some("/api/v2/users"),
        );
        assert!(pattern.matches(&addr2));

        // path 不匹配
        let addr3 = create_address(Protocol::Http, "example.com", Some(80), Some("/other"));
        assert!(!pattern.matches(&addr3));

        // host 前缀匹配
        let pattern2 = AddressPattern::new(Protocol::Https, "*.example.com", None, None).unwrap();

        let addr4 = create_address(Protocol::Https, "api.example.com", Some(443), None);
        assert!(pattern2.matches(&addr4));

        let addr5 = create_address(Protocol::Https, "www.example.com", Some(443), None);
        assert!(pattern2.matches(&addr5));

        let addr6 = create_address(Protocol::Https, "other.com", Some(443), None);
        assert!(!pattern2.matches(&addr6));
    }

    #[test]
    fn test_regex_match() {
        // host 使用正则匹配
        let pattern =
            AddressPattern::new(Protocol::Http, "re:.*\\.example\\.com", None, None).unwrap();

        let addr1 = create_address(Protocol::Http, "api.example.com", Some(80), None);
        assert!(pattern.matches(&addr1));

        let addr2 = create_address(Protocol::Http, "www.example.com", Some(80), None);
        assert!(pattern.matches(&addr2));

        let addr3 = create_address(Protocol::Http, "example.com", Some(80), None);
        assert!(!pattern.matches(&addr3)); // 不匹配（因为没有前面的子域名）

        let addr4 = create_address(Protocol::Http, "other.com", Some(80), None);
        assert!(!pattern.matches(&addr4));

        // path 使用正则匹配
        let pattern2 = AddressPattern::new(
            Protocol::Http,
            "api.example.com",
            None,
            Some("re:/api/v[0-9]+/.*"),
        )
        .unwrap();

        let addr5 = create_address(
            Protocol::Http,
            "api.example.com",
            None,
            Some("/api/v1/users"),
        );
        assert!(pattern2.matches(&addr5));

        let addr6 = create_address(
            Protocol::Http,
            "api.example.com",
            None,
            Some("/api/v2/posts"),
        );
        assert!(pattern2.matches(&addr6));

        let addr7 = create_address(Protocol::Http, "api.example.com", None, Some("/api/users"));
        assert!(!pattern2.matches(&addr7));
    }

    #[test]
    fn test_port_matching() {
        // 模式指定端口
        let pattern = AddressPattern::new(Protocol::Http, "example.com", Some(8080), None).unwrap();

        let addr1 = create_address(Protocol::Http, "example.com", Some(8080), None);
        assert!(pattern.matches(&addr1));

        let addr2 = create_address(Protocol::Http, "example.com", Some(80), None);
        assert!(!pattern.matches(&addr2));

        // 模式不指定端口（任意端口都匹配）
        let pattern2 = AddressPattern::new(Protocol::Http, "example.com", None, None).unwrap();

        let addr3 = create_address(Protocol::Http, "example.com", Some(80), None);
        assert!(pattern2.matches(&addr3));

        let addr4 = create_address(Protocol::Http, "example.com", Some(8080), None);
        assert!(pattern2.matches(&addr4));

        let addr5 = create_address(Protocol::Http, "example.com", None, None);
        assert!(pattern2.matches(&addr5));
    }

    #[test]
    fn test_mixed_pattern() {
        // host 正则 + path 前缀
        let pattern = AddressPattern::new(
            Protocol::Https,
            "re:.*\\.api\\.com",
            Some(443),
            Some("/v1/*"),
        )
        .unwrap();

        let addr1 = create_address(
            Protocol::Https,
            "prod.api.com",
            Some(443),
            Some("/v1/users"),
        );
        assert!(pattern.matches(&addr1));

        let addr2 = create_address(
            Protocol::Https,
            "test.api.com",
            Some(443),
            Some("/v1/posts"),
        );
        assert!(pattern.matches(&addr2));

        let addr3 = create_address(
            Protocol::Https,
            "prod.api.com",
            Some(443),
            Some("/v2/users"),
        );
        assert!(!pattern.matches(&addr3));
    }

    #[test]
    fn test_invalid_regex() {
        // 无效的正则表达式应该返回错误
        let result = AddressPattern::new(Protocol::Http, "re:[invalid(regex", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_address_from_uri() {
        // 测试从 Uri 创建 Address
        let uri: Uri = "http://example.com:8080/api/v1?key=value".parse().unwrap();
        let address = Address::from_uri(&uri).unwrap();

        assert_eq!(address.protocol, Protocol::Http);
        assert_eq!(address.host, "example.com");
        assert_eq!(address.port, Some(8080));
        assert_eq!(address.path, Some("/api/v1?key=value".to_string()));
    }

    #[test]
    fn test_address_path_rewrite_modes() {
        use proxy_fork_core::PathTransformMode;

        // 测试 1: Preserve 模式 - 完全保留原始路径
        let target = Address {
            protocol: Protocol::Http,
            host: "localhost".to_string(),
            port: Some(5001),
            path: None,
            path_transform_mode: PathTransformMode::Preserve,
        };

        // 原始 URI 包含完整路径
        let original_uri: Uri = "https://api.example.com/console/api/open/logo?v=1"
            .parse()
            .unwrap();

        // 转换后应该保留原始路径和查询参数，但改变 scheme、host 和 port
        let new_uri = target.to_uri_with_rewrite(&original_uri, None).unwrap();

        assert_eq!(new_uri.scheme_str(), Some("http"));
        assert_eq!(new_uri.host(), Some("localhost"));
        assert_eq!(new_uri.port_u16(), Some(5001));
        assert_eq!(
            new_uri.path_and_query().unwrap().as_str(),
            "/console/api/open/logo?v=1"
        );

        // 测试 2: Prepend 模式 - 前缀拼接
        let target2 = Address {
            protocol: Protocol::Https,
            host: "backend.example.com".to_string(),
            port: Some(8080),
            path: Some("/local".to_string()),
            path_transform_mode: PathTransformMode::Prepend,
        };

        let original_uri2: Uri = "http://example.com/test/path?key=value".parse().unwrap();
        let new_uri2 = target2.to_uri_with_rewrite(&original_uri2, None).unwrap();

        assert_eq!(new_uri2.scheme_str(), Some("https"));
        assert_eq!(new_uri2.host(), Some("backend.example.com"));
        assert_eq!(new_uri2.port_u16(), Some(8080));
        // 应该将 target.path 作为前缀拼接到原始路径前面
        assert_eq!(
            new_uri2.path_and_query().unwrap().as_str(),
            "/local/test/path?key=value"
        );

        // 测试 3: Prepend 模式 - path 前缀带尾部斜杠
        let target3 = Address {
            protocol: Protocol::Http,
            host: "localhost".to_string(),
            port: Some(5001),
            path: Some("/local/".to_string()),
            path_transform_mode: PathTransformMode::Prepend,
        };

        let original_uri3: Uri = "https://api.example.com/console/api/open/logo"
            .parse()
            .unwrap();
        let new_uri3 = target3.to_uri_with_rewrite(&original_uri3, None).unwrap();

        // 应该自动去掉前缀的尾部斜杠，避免双斜杠
        assert_eq!(
            new_uri3.path_and_query().unwrap().as_str(),
            "/local/console/api/open/logo"
        );

        // 测试 4: Replace 模式 - 路径前缀替换
        let target4 = Address {
            protocol: Protocol::Https,
            host: "api.example.com".to_string(),
            port: None,
            path: Some("/console/api/v2".to_string()),
            path_transform_mode: PathTransformMode::Replace,
        };

        let original_uri4: Uri = "https://api.example.com/console/api/open/logo"
            .parse()
            .unwrap();
        let new_uri4 = target4
            .to_uri_with_rewrite(&original_uri4, Some("/console/api"))
            .unwrap();

        assert_eq!(
            new_uri4.to_string(),
            "https://api.example.com/console/api/v2/open/logo"
        );
    }
}
