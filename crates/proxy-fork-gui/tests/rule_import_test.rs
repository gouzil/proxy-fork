use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{App, Page},
    preferences::Preferences,
    rule_import::{load_files, load_source, parse_rules},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const RULE: &str = r#"
protocol = "https"
host = "*.example.com"
path = "/api/*"
target_host = "127.0.0.1"
target_port = 8080
path_transform = "replace"
target_path = "/v2"
"#;

#[test]
fn parses_native_configuration_and_standalone_rules_without_importing_settings() {
    for prefix in [
        "listen = 'invalid-but-not-imported'\nnoca = false\n[[proxy_manager.rules]]\n",
        "[[rules]]\n",
        "\u{feff}[[rules]]\n",
    ] {
        let rules = parse_rules(format!("{prefix}{RULE}").as_bytes()).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].host, "*.example.com");
        assert_eq!(rules[0].port, None);
        assert_eq!(rules[0].target_port, Some(8080));
        assert_eq!(rules[0].target_protocol, None);
        assert_eq!(rules[0].target_path.as_deref(), Some("/v2"));
    }
}

#[test]
fn rejects_empty_malformed_or_partially_invalid_imports() {
    for source in [
        String::new(),
        "rules = []".into(),
        "[proxy_manager]\ncache_size = 1000".into(),
        "<html>not a rules file</html>".into(),
        format!(
            "[[rules]]\n{RULE}\n[[rules]]\n{}",
            RULE.replace("8080", "0")
        ),
        format!("[[rules]]\n{}", RULE.replace("*.example.com", "re:[")),
        format!("[[rules]]\n{}", RULE.replace("127.0.0.1", "")),
    ] {
        assert!(parse_rules(source.as_bytes()).is_err(), "{source}");
    }
    assert!(parse_rules(&[0xff]).is_err());
    assert!(
        parse_rules(&vec![b' '; 1024 * 1024 + 1])
            .unwrap_err()
            .to_string()
            .contains("exceeds 1 MiB")
    );
}

#[tokio::test]
async fn loads_multiple_dropped_files_in_order_and_local_paths_with_spaces() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("规则 one.toml");
    let second = temp.path().join("two.toml");
    fs_err::write(&first, format!("[[rules]]\n{RULE}")).unwrap();
    fs_err::write(
        &second,
        format!("[[rules]]\n{}", RULE.replace("8080", "9090")),
    )
    .unwrap();
    let rules = load_files(vec![first.clone(), second]).await.unwrap();
    assert_eq!(
        rules.iter().map(|r| r.target_port).collect::<Vec<_>>(),
        [Some(8080), Some(9090)]
    );
    assert_eq!(
        load_source(first.to_str().unwrap()).await.unwrap(),
        rules[..1]
    );
}

#[tokio::test]
async fn file_import_rejects_invalid_batches_missing_files_directories_and_large_files() {
    let temp = tempfile::tempdir().unwrap();
    let valid = temp.path().join("valid.toml");
    let invalid = temp.path().join("invalid.toml");
    fs_err::write(&valid, format!("[[rules]]\n{RULE}")).unwrap();
    fs_err::write(&invalid, "invalid").unwrap();
    assert!(load_files(vec![valid, invalid.clone()]).await.is_err());
    assert!(load_files(vec![temp.path().join("missing")]).await.is_err());
    assert!(load_files(vec![temp.path().into()]).await.is_err());
    fs_err::File::create(&invalid)
        .unwrap()
        .set_len(1024 * 1024 + 1)
        .unwrap();
    assert!(
        load_files(vec![invalid])
            .await
            .unwrap_err()
            .to_string()
            .contains("exceeds 1 MiB")
    );
    assert!(load_files(Vec::new()).await.is_err());
    assert!(load_source("  ").await.is_err());
    assert!(load_source("ftp://example.com/rules.toml").await.is_err());
}

async fn serve(response: String) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        let _ = stream.write_all(response.as_bytes()).await;
    });
    format!("http://{address}/rules.toml")
}

#[tokio::test]
async fn downloads_rules_from_an_http_address() {
    let body = format!("[[proxy_manager.rules]]\n{RULE}");
    let url = serve(format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ))
    .await;
    let rules = load_source(&format!("  {url}  ")).await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].host, "*.example.com");
}

#[tokio::test]
async fn rejects_http_errors_html_and_oversized_chunked_responses() {
    let body = format!("[[rules]]\n{RULE}");
    let url = serve(format!(
        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ))
    .await;
    assert!(
        load_source(&url)
            .await
            .unwrap_err()
            .to_string()
            .contains("404")
    );
    for response in [
        "HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\n<html></html>".to_string(),
        "HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\n\r\n".to_string(),
        format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n100001\r\n{}\r\n0\r\n\r\n",
            " ".repeat(1024 * 1024 + 1)
        ),
    ] {
        let url = serve(response).await;
        assert!(load_source(&url).await.is_err());
    }
}

#[test]
fn imported_rules_keep_existing_ids_and_are_marked_saved_only_after_persistence() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.add_rule();
    app.rules[0].rule.host = "existing.example".into();
    let old_id = app.rules[0].id;
    let imported = parse_rules(format!("[[rules]]\n{RULE}").as_bytes()).unwrap();
    let mut saved = app.edited_rules().unwrap();
    saved.extend(imported);
    app.rule_import_dialog = true;
    app.rule_import_files.push("rules.toml".into());
    app.rules_imported(saved.clone());
    assert_eq!(app.rules[0].id, old_id);
    assert_eq!(app.rules[0].rule.host, "existing.example");
    assert_ne!(app.rules[1].id, old_id);
    assert_eq!(app.rules[1].target_port, "8080");
    assert_eq!(app.edited_rules().unwrap(), saved);
    assert_eq!(app.config.proxy_manager.rules, saved);
    assert!(!app.rules_dirty);
    assert!(!app.rule_import_dialog);
    assert!(app.rule_import_files.is_empty());
    assert!(!app.config_dirty);
}

#[test]
fn dropping_files_opens_import_and_preserves_rules_until_submitted() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    assert!(app.drop_rule_file("one.toml".into()));
    assert!(app.drop_rule_file("two.toml".into()));
    assert_eq!(app.page, Page::Rules);
    assert!(app.rule_import_dialog);
    assert_eq!(app.rule_import_files.len(), 2);
    assert!(app.rules.is_empty());
    app.busy = true;
    assert!(!app.drop_rule_file("busy.toml".into()));
    app.busy = false;
    app.exiting = true;
    assert!(!app.drop_rule_file("exiting.toml".into()));
    assert_eq!(app.rule_import_files.len(), 2);
}
