use http::Uri;
use tracing::debug;

// 去除 URL 中的默认端口
pub fn remove_default_ports(uri: &Uri) -> Option<Uri> {
    let scheme = uri.scheme_str().unwrap_or("http");
    let host = uri.host().unwrap_or("");
    let port = uri.port_u16();
    let include_port = match (scheme, port) {
        ("http", Some(80)) => true,
        ("https", Some(443)) => true,
        ("http", Some(443)) => true,
        (path_url, Some(i)) => {
            debug!("url: {path_url}  include_port: {i}");
            false
        }
        (_, None) => false,
    };
    if include_port {
        return Some(
            Uri::builder()
                .scheme(scheme)
                .authority(host)
                .path_and_query(uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/"))
                .build()
                .unwrap(),
        );
    }
    None
}
