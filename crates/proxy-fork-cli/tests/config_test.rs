#[cfg(test)]
mod tests {
    use proxy_fork_cli::config::*;

    #[test]
    fn test_listen_addr_default() {
        let listen = ListenAddrBuilder::default().build().unwrap();
        assert_eq!(listen.host, "127.0.0.1");
        assert_eq!(listen.port, 7898);
    }

    #[test]
    fn test_app_config_default() {
        let cfg = AppConfigBuilder::default()
            .listen(ListenAddrBuilder::default().build().unwrap())
            .proxy_manager(ProxyManagerRuntimeBuilder::default().build().unwrap())
            .build()
            .unwrap();
        assert!(cfg.enable_ca);
    }

    #[test]
    fn test_split_host_port() {
        let (h, p) = split_host_port("0.0.0.0:9999").unwrap();
        assert_eq!(h, "0.0.0.0");
        assert_eq!(p, 9999);
        assert!(split_host_port("bad").is_none());
    }

    #[test]
    fn explicit_config_and_cli_overlays_keep_rule_order() {
        use proxy_fork_cli::{
            args::{GlobalConfigArgs, StartProxyArgs},
            config::load_start_proxy_config,
        };
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("explicit.toml");
        fs_err::write(&path, "listen = '127.0.0.1:9876'\nnoca = false\n[proxy_manager]\ncache_size = 42\n[[proxy_manager.rules]]\nprotocol = 'https'\nhost = 'first.example'\ntarget_host = 'localhost'\n").unwrap();
        let global = GlobalConfigArgs {
            config: Some(path),
            debug: 2,
        };
        let mut args = StartProxyArgs {
            listen: Some("localhost:12345".into()),
            noca: true,
            ..Default::default()
        };
        args.rules.push(proxy_fork_cli::args::RuleItem {
            protocol: "http".into(),
            host: "second.example".into(),
            target_host: "localhost".into(),
            ..Default::default()
        });
        let cfg = load_start_proxy_config(&global, &args).unwrap();
        assert_eq!(cfg.listen.port, 12345);
        assert_eq!(cfg.listen.host, "localhost");
        assert!(!cfg.enable_ca);
        assert_eq!(cfg.debug, 2);
        assert_eq!(cfg.proxy_manager.cache_size, 42);
        assert_eq!(cfg.proxy_manager.rules[0].host, "first.example");
        assert_eq!(
            cfg.proxy_manager.rules.last().unwrap().host,
            "second.example"
        );
    }

    #[test]
    fn cli_keeps_skipping_invalid_rules_while_shared_gui_validation_remains_strict() {
        use proxy_fork_cli::args::{GlobalConfigArgs, StartProxyArgs};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs_err::write(&path, "[[proxy_manager.rules]]\nprotocol = 'ws'\nhost = 'invalid.example'\ntarget_host = 'localhost'\n[[proxy_manager.rules]]\nprotocol = 'https'\nhost = 'valid.example'\ntarget_host = 'localhost'\n").unwrap();
        let global = GlobalConfigArgs {
            config: Some(path),
            debug: 0,
        };
        let cfg = load_start_proxy_config(&global, &StartProxyArgs::default()).unwrap();
        assert_eq!(cfg.proxy_manager.rules.len(), 1);
        assert_eq!(cfg.proxy_manager.rules[0].host, "valid.example");
    }
}
