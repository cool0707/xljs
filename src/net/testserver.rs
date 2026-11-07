//! A small HTTP server for the tests of fetch (127.0.0.1, a port of its
//! own; one thread per connection, Connection: close). Routes:
//!
//! - `/echo...`: the request as JSON {method, path, headers, body, length}
//! - `/status/<n>`: status n
//! - `/redirect/<n>`: n redirects (302) before /echo
//! - `/redirect-to?status=<n>&url=<url>`: a redirect to url
//! - `/sjis`: text/plain in Shift_JIS
//! - `/chunked`: a chunked body
//! - `/json`: a JSON body
//! - `/cookies`: two Set-Cookie headers
//! - `/slow`: answers after 3 seconds
//! - `/auth`: 401 with Basic unless the request has Authorization

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::LazyLock;
use std::time::Duration;

/// The base URL of the server ("http://127.0.0.1:<port>").
pub(crate) fn url() -> &'static str {
    static URL: LazyLock<String> = LazyLock::new(|| {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || {
                    let _ = serve(s);
                });
            }
        });
        format!("http://127.0.0.1:{port}")
    });
    &URL
}

fn query<'a>(path: &'a str, key: &str) -> Option<String> {
    let q = path.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| unescape(v))
    })
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn serve(s: TcpStream) -> std::io::Result<()> {
    let mut r = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let mut parts = line.trim_end().splitn(3, ' ');
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers = serde_json::Map::new();
    let mut len = 0usize;
    loop {
        let mut l = String::new();
        r.read_line(&mut l)?;
        let l = l.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if k == "content-length" {
                len = v.parse().unwrap_or(0);
            }
            headers.insert(k, v.into());
        }
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    let mut w = s;
    let reply = |w: &mut TcpStream, status: &str, extra: &[(&str, &str)], body: &[u8]| -> std::io::Result<()> {
        let mut head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
        for (k, v) in extra {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str("\r\n");
        w.write_all(head.as_bytes())?;
        if method != "HEAD" {
            w.write_all(body)?;
        }
        w.flush()
    };
    let route = path.split('?').next().unwrap_or("");
    if route.starts_with("/echo") {
        let j = serde_json::json!({
            "method": method,
            "path": path,
            "headers": headers,
            "body": String::from_utf8_lossy(&body),
            "length": body.len(),
        });
        return reply(&mut w, "200 OK", &[("Content-Type", "application/json")], j.to_string().as_bytes());
    }
    if let Some(n) = route.strip_prefix("/status/") {
        let n: u16 = n.parse().unwrap_or(500);
        let text = match n {
            404 => "Not Found",
            500 => "Internal Server Error",
            201 => "Created",
            204 => "No Content",
            _ => "Status",
        };
        let b = if n == 204 { String::new() } else { format!("status {n}") };
        return reply(&mut w, &format!("{n} {text}"), &[("Content-Type", "text/plain")], b.as_bytes());
    }
    if let Some(n) = route.strip_prefix("/redirect/") {
        let n: u32 = n.parse().unwrap_or(0);
        let to = if n <= 1 { "/echo".to_string() } else { format!("/redirect/{}", n - 1) };
        return reply(&mut w, "302 Found", &[("Location", &to)], b"");
    }
    if route == "/redirect-to" {
        let status = query(&path, "status").unwrap_or_else(|| "302".into());
        let to = query(&path, "url").unwrap_or_default();
        return reply(&mut w, &format!("{status} Redirect"), &[("Location", &to)], b"");
    }
    match route {
        "/sjis" => {
            let (b, _, _) = encoding_rs::SHIFT_JIS.encode("日本語のテキスト");
            reply(&mut w, "200 OK", &[("Content-Type", "text/plain; charset=Shift_JIS")], &b)
        }
        "/chunked" => {
            w.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n")?;
            for part in ["こんにちは", "、", "世界"] {
                w.write_all(format!("{:x}\r\n{part}\r\n", part.len()).as_bytes())?;
            }
            w.write_all(b"0\r\n\r\n")?;
            w.flush()
        }
        "/json" => reply(&mut w, "200 OK", &[("Content-Type", "application/json; charset=utf-8")], r#"{"a":1,"b":"あ"}"#.as_bytes()),
        "/cookies" => reply(&mut w, "200 OK", &[("Set-Cookie", "a=1; Path=/"), ("Set-Cookie", "b=2; Path=/")], b"ok"),
        "/slow" => {
            std::thread::sleep(Duration::from_secs(3));
            reply(&mut w, "200 OK", &[], b"slow")
        }
        "/auth" => {
            if headers.contains_key("authorization") {
                let a = headers["authorization"].as_str().unwrap_or("").to_string();
                reply(&mut w, "200 OK", &[], a.as_bytes())
            } else {
                reply(&mut w, "401 Unauthorized", &[("WWW-Authenticate", "Basic realm=\"test\"")], b"no")
            }
        }
        _ => reply(&mut w, "404 Not Found", &[], b"not found"),
    }
}
