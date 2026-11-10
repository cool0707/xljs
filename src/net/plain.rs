//! Plain HTTP/1.1 over a socket (not Windows; for the tests). HTTPS is not
//! supported: xljs runs on Windows, where WinHTTP does it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};

use super::auth::{self, Login, LoginPrompt};
use super::{CONNECT_TIMEOUT, Cancel, IO_TIMEOUT, Request, Response, Target, parse_headers, parse_status};
use crate::{Error, Result, err};

fn io_err(req: &Request, e: std::io::Error) -> Error {
    match e.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => err!("net.timeout", req.url),
        _ => err!("net.failed", req.url, e),
    }
}

/// Sends the request, through the proxy of XLJS_PROXY if set (the tests
/// of Basic proxy authentication: 407 asks with Request::ask).
pub(super) fn send(req: &Request, t: &Target, cancel: &Cancel) -> Result<Response> {
    let proxy = super::proxy_override();
    let mut login: Option<(String, Login)> = None;
    let mut prompts = 0;
    loop {
        let res = send_once(req, t, cancel, proxy.as_deref(), login.as_ref().map(|x| &x.1))?;
        if res.status != 407 {
            if let Some((realm, l)) = &login {
                auth::confirm(&auth::Key::proxy(realm), l);
            }
            return Ok(res);
        }
        let realm = res.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("proxy-authenticate")).find_map(|(_, v)| auth::basic_realm(v));
        let (Some(realm), Some(ask)) = (realm, &req.ask) else { return Ok(res) };
        let failed = login.take().map(|(_, l)| l);
        if failed.is_some() {
            prompts += 1;
            if prompts >= 5 {
                return Ok(res);
            }
        }
        let prompt = LoginPrompt { proxy: true, host: proxy.clone().unwrap_or_default(), ..Default::default() };
        match auth::login(&auth::Key::proxy(&realm), prompt, failed.as_ref(), ask) {
            Some(l) => login = Some((realm, l)),
            None => return Ok(res),
        }
    }
}

fn send_once(req: &Request, t: &Target, cancel: &Cancel, proxy: Option<&str>, login: Option<&Login>) -> Result<Response> {
    if t.secure {
        return Err(err!("net.httpsUnsupported"));
    }
    let (host, port) = match proxy {
        Some(p) => {
            let (h, port) = p.rsplit_once(':').ok_or_else(|| err!("net.badUrl", p))?;
            (h.to_string(), port.parse::<u16>().map_err(|_| err!("net.badUrl", p))?)
        }
        None => (t.host.clone(), t.port),
    };
    let addrs = (host.as_str(), port).to_socket_addrs().map_err(|_| err!("net.unresolved", req.url))?;
    let mut last = None;
    let mut stream = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, CONNECT_TIMEOUT) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let Some(mut s) = stream else {
        return Err(match last {
            Some(e) if e.kind() == std::io::ErrorKind::TimedOut => err!("net.timeout", req.url),
            Some(e) => err!("net.cannotConnect", req.url, e),
            None => err!("net.unresolved", req.url),
        });
    };
    let timeout = req.timeout.unwrap_or(IO_TIMEOUT);
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(timeout));
    if let Ok(c) = s.try_clone() {
        if !cancel.set_closer(move || {
            let _ = c.shutdown(Shutdown::Both);
        }) {
            return Err(err!("net.failed", req.url, "aborted"));
        }
    }

    let target = if proxy.is_some() { format!("http://{}{}", t.host_header(), t.path) } else { t.path.clone() };
    let mut head = format!("{} {target} HTTP/1.1\r\nHost: {}\r\n", req.method, t.host_header());
    if let Some(l) = login {
        use base64::Engine as _;
        let b = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", l.user, l.password));
        head.push_str(&format!("Proxy-Authorization: Basic {b}\r\n"));
    }
    let has = |n: &str| req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    if !has("user-agent") {
        head.push_str(&format!("User-Agent: {}\r\n", req.agent));
    }
    for (k, v) in &req.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    let body = req.body.as_deref().unwrap_or(&[]);
    if req.body.is_some() || !matches!(req.method.as_str(), "GET" | "HEAD" | "OPTIONS" | "DELETE") {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("Connection: close\r\n\r\n");
    let mut data = head.into_bytes();
    data.extend_from_slice(body);
    s.write_all(&data).map_err(|e| io_err(req, e))?;
    s.flush().map_err(|e| io_err(req, e))?;

    let mut r = BufReader::new(s);
    let read_line = |r: &mut BufReader<TcpStream>| -> Result<String> {
        let mut buf = Vec::new();
        r.read_until(b'\n', &mut buf).map_err(|e| io_err(req, e))?;
        if buf.is_empty() {
            return Err(err!("net.failed", req.url, "connection closed"));
        }
        Ok(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string())
    };
    // Interim responses (100 Continue) come before the real one.
    let (status, status_text, headers) = loop {
        let line = read_line(&mut r)?;
        let (code, text) = parse_status(&line).ok_or_else(|| err!("net.failed", req.url, "bad status line"))?;
        let mut lines = Vec::new();
        loop {
            let l = read_line(&mut r)?;
            if l.is_empty() {
                break;
            }
            lines.push(l);
        }
        if (100..200).contains(&code) {
            continue;
        }
        break (code, text, parse_headers(lines.iter().map(|s| s.as_str())));
    };
    let header = |n: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.as_str());
    let mut body = Vec::new();
    let no_body = req.method == "HEAD" || status == 204 || status == 304;
    if no_body {
    } else if header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
        loop {
            let line = read_line(&mut r)?;
            let size = usize::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| err!("net.failed", req.url, "bad chunk"))?;
            if size == 0 {
                // Trailers up to the blank line.
                while !read_line(&mut r)?.is_empty() {}
                break;
            }
            let start = body.len();
            body.resize(start + size, 0);
            r.read_exact(&mut body[start..]).map_err(|e| io_err(req, e))?;
            read_line(&mut r)?;
        }
    } else if let Some(n) = header("content-length").and_then(|v| v.trim().parse::<usize>().ok()) {
        body.resize(n, 0);
        r.read_exact(&mut body).map_err(|e| io_err(req, e))?;
    } else {
        r.read_to_end(&mut body).map_err(|e| io_err(req, e))?;
    }
    if cancel.cancelled() {
        return Err(err!("net.failed", req.url, "aborted"));
    }
    Ok(Response { status, status_text, headers, body })
}
