use super::*;

#[test]
fn target_parse() {
    let t = Target::parse("http://example.com").unwrap();
    assert_eq!(
        t,
        Target {
            secure: false,
            host: "example.com".into(),
            port: 80,
            path: "/".into()
        }
    );
    let t = Target::parse("HTTPS://user:pw@example.com:8443/a/b?x=1#frag").unwrap();
    assert_eq!(
        t,
        Target {
            secure: true,
            host: "example.com".into(),
            port: 8443,
            path: "/a/b?x=1".into()
        }
    );
    assert_eq!(t.host_header(), "example.com:8443");
    let t = Target::parse("http://[::1]:8080?q").unwrap();
    assert_eq!(
        (t.host.as_str(), t.port, t.path.as_str()),
        ("::1", 8080, "/?q")
    );
    assert_eq!(t.host_header(), "[::1]:8080");
    assert_eq!(
        Target::parse("https://example.com/").unwrap().host_header(),
        "example.com"
    );
    assert!(Target::parse("ftp://example.com/").is_err());
    assert!(Target::parse("example.com").is_err());
    assert!(Target::parse("http:///x").is_err());
    assert!(Target::parse("http://h:99999/").is_err());
}

#[test]
fn status_and_headers() {
    assert_eq!(
        parse_status("HTTP/1.1 404 Not Found"),
        Some((404, "Not Found".into()))
    );
    assert_eq!(parse_status("HTTP/2 200"), Some((200, String::new())));
    assert_eq!(parse_status("garbage"), None);
    let h = parse_headers(["A: 1", "B:2 ", "  continued", "", "C: 3"].into_iter());
    assert_eq!(
        h,
        vec![("A".into(), "1".into()), ("B".into(), "2 continued".into())]
    );
}

#[test]
fn cancel_runs_closer_once() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let c = Cancel::default();
    assert!(c.set_closer(|| {
        N.fetch_add(1, Ordering::SeqCst);
    }));
    c.abort();
    c.finish();
    c.abort();
    assert_eq!(N.load(Ordering::SeqCst), 1);
    assert!(c.cancelled());
    // Aborted before the request registered: the closer runs at once.
    assert!(!c.set_closer(|| {
        N.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(N.load(Ordering::SeqCst), 2);
}

#[cfg(not(windows))]
fn get(path: &str) -> Result<Response> {
    let req = Request {
        method: "GET".into(),
        url: format!("{}{path}", testserver::url()),
        headers: vec![("X-Test".into(), "1".into())],
        body: None,
        timeout: None,
        credentials: Credentials::SameOrigin,
        agent: "xljs/test".into(),
        ask: None,
    };
    send(&req, &Cancel::default())
}

#[cfg(not(windows))]
#[test]
fn plain_requests() {
    let r = get("/echo?a=1").unwrap();
    assert_eq!(r.status, 200);
    let j: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(j["path"], "/echo?a=1");
    assert_eq!(j["headers"]["x-test"], "1");
    assert_eq!(j["headers"]["user-agent"], "xljs/test");
    let r = get("/chunked").unwrap();
    assert_eq!(String::from_utf8(r.body).unwrap(), "こんにちは、世界");
    let r = get("/status/404").unwrap();
    assert_eq!((r.status, r.status_text.as_str()), (404, "Not Found"));
    let r = get("/redirect/1").unwrap();
    assert_eq!(r.status, 302);
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| k == "Location" && v == "/echo")
    );
    let r = get("/cookies").unwrap();
    assert_eq!(
        r.headers.iter().filter(|(k, _)| k == "Set-Cookie").count(),
        2
    );
}

#[cfg(not(windows))]
#[test]
fn plain_errors_and_abort() {
    let _l = crate::testutil::lock_lang("en");
    let mut req = Request {
        method: "GET".into(),
        url: "https://example.com/".into(),
        headers: vec![],
        body: None,
        timeout: None,
        credentials: Credentials::SameOrigin,
        agent: "xljs/test".into(),
        ask: None,
    };
    assert!(send(&req, &Cancel::default()).is_err());
    // Nothing listens on the port.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    req.url = format!("http://127.0.0.1:{port}/");
    assert!(send(&req, &Cancel::default()).is_err());
    req.url = format!("{}/slow", testserver::url());
    req.timeout = Some(Duration::from_millis(200));
    let e = send(&req, &Cancel::default()).unwrap_err();
    assert_eq!(e.key(), Some("net.timeout"));
    req.timeout = None;
    let c = Cancel::default();
    let c2 = c.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        c2.abort();
    });
    let t = std::time::Instant::now();
    assert!(send(&req, &c).is_err());
    assert!(t.elapsed() < Duration::from_secs(2));
}
