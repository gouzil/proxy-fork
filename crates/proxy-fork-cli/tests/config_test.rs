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
        assert_eq!(cfg.enable_ca, true);
    }

    #[test]
    fn test_split_host_port() {
        let (h, p) = split_host_port("0.0.0.0:9999").unwrap();
        assert_eq!(h, "0.0.0.0");
        assert_eq!(p, 9999);
        assert!(split_host_port("bad").is_none());
    }
}
