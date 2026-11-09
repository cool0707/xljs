//! HTTP requests for fetch (the natives are in src/api/net.rs). One call
//! is one request without redirects: the prelude follows them, builds the
//! Request and Response objects and handles AbortSignal.
//!
//! On Windows the requests go through WinHTTP (`winhttp.rs`): the proxy of
//! the system (including PAC), Windows authentication (Negotiate / NTLM),
//! cookies within the run, TLS with the certificate store of Windows and
//! gzip. Elsewhere (`plain.rs`, for the tests) only plain HTTP/1.1.

pub mod agent;
pub mod auth;
#[cfg(not(windows))]
mod plain;
#[cfg(windows)]
mod winhttp;

#[cfg(test)]
pub(crate) mod testserver;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Result, err};

/// Whether Windows authentication and cookies are used (fetch's
/// `credentials`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credentials {
    /// Neither cookies nor Windows authentication.
    Omit,
    /// The default: Windows authentication only for intranet servers.
    SameOrigin,
    /// Windows authentication for every server.
    Include,
}

impl Credentials {
    pub fn parse(s: &str) -> Credentials {
        match s {
            "omit" => Credentials::Omit,
            "include" => Credentials::Include,
            _ => Credentials::SameOrigin,
        }
    }
}

#[derive(Clone)]
pub struct Request {
    pub method: String,
    /// An absolute http(s) URL (already encoded by the prelude's URL).
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    /// The time a send or a receive may take (None: the default).
    pub timeout: Option<Duration>,
    pub credentials: Credentials,
    /// The User-Agent unless the headers have one (agent::for_fetch).
    pub agent: String,
    /// Asks for the login of a proxy that wants Basic authentication (none:
    /// the 407 is the response).
    pub ask: Option<auth::Ask>,
}

#[derive(Debug, Clone, Default)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The proxy of the environment variable XLJS_PROXY ("host:port"), used
/// instead of the system's.
pub(crate) fn proxy_override() -> Option<String> {
    let v = std::env::var("XLJS_PROXY").ok()?;
    let v = v.trim();
    let v = v.strip_prefix("http://").unwrap_or(v).trim_end_matches('/');
    (!v.is_empty()).then(|| v.to_string())
}

/// The time a connection may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// The default time a send or a receive may take.
pub const IO_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Default)]
struct CancelState {
    cancelled: bool,
    closer: Option<Box<dyn FnOnce() + Send>>,
}

/// Aborts a request from another thread: the request registers how to
/// interrupt it (close its handle or socket), abort runs that.
#[derive(Clone, Default)]
pub struct Cancel(Arc<Mutex<CancelState>>);

impl Cancel {
    pub fn abort(&self) {
        let f = {
            let mut st = self.0.lock().unwrap();
            st.cancelled = true;
            st.closer.take()
        };
        if let Some(f) = f {
            f();
        }
    }

    pub fn cancelled(&self) -> bool {
        self.0.lock().unwrap().cancelled
    }

    /// Sets how to interrupt the request. When it is aborted already, f
    /// runs at once and the result is false.
    pub(crate) fn set_closer(&self, f: impl FnOnce() + Send + 'static) -> bool {
        let mut st = self.0.lock().unwrap();
        if st.cancelled {
            drop(st);
            f();
            return false;
        }
        st.closer = Some(Box::new(f));
        true
    }

    /// Runs the closer when nothing ran it yet (the end of the request);
    /// whoever takes it runs it, so it runs once.
    pub(crate) fn finish(&self) {
        let f = self.0.lock().unwrap().closer.take();
        if let Some(f) = f {
            f();
        }
    }
}

/// The parts of an absolute http(s) URL.
#[derive(Debug, PartialEq)]
pub(crate) struct Target {
    pub secure: bool,
    /// The host name (an IPv6 address without the brackets).
    pub host: String,
    pub port: u16,
    /// The path with the query ("/" at least), without the fragment.
    pub path: String,
}

impl Target {
    pub fn parse(url: &str) -> Result<Target> {
        let bad = || err!("net.badUrl", url);
        let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
        let secure = match scheme.to_ascii_lowercase().as_str() {
            "http" => false,
            "https" => true,
            _ => return Err(err!("net.badScheme", url)),
        };
        let rest = rest.split('#').next().unwrap_or("");
        let end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (auth, path) = rest.split_at(end);
        let hostport = auth.rsplit('@').next().unwrap_or("");
        let (host, port) = if let Some(h) = hostport.strip_prefix('[') {
            let (h, p) = h.split_once(']').ok_or_else(bad)?;
            (h.to_string(), p.strip_prefix(':').unwrap_or(""))
        } else {
            match hostport.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), p),
                None => (hostport.to_string(), ""),
            }
        };
        if host.is_empty() {
            return Err(bad());
        }
        let port = if port.is_empty() {
            if secure { 443 } else { 80 }
        } else {
            port.parse().map_err(|_| bad())?
        };
        let path = if path.is_empty() {
            "/".to_string()
        } else if path.starts_with('?') {
            format!("/{path}")
        } else {
            path.to_string()
        };
        Ok(Target {
            secure,
            host,
            port,
            path,
        })
    }

    /// The Host header (WinHTTP makes its own).
    #[cfg_attr(all(windows, not(test)), allow(dead_code))]
    pub fn host_header(&self) -> String {
        let h = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == if self.secure { 443 } else { 80 } {
            h
        } else {
            format!("{h}:{}", self.port)
        }
    }
}

/// Sends the request and reads the whole response. An aborted request
/// fails with some error (the caller knows that it aborted it).
pub fn send(req: &Request, cancel: &Cancel) -> Result<Response> {
    let t = Target::parse(&req.url)?;
    #[cfg(windows)]
    let r = winhttp::send(req, &t, cancel);
    #[cfg(not(windows))]
    let r = plain::send(req, &t, cancel);
    cancel.finish();
    r
}

/// Parses the header lines of a response (after the status line).
pub(crate) fn parse_headers<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            // An obsolete folded line continues the previous value.
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

/// The status code and text of a status line ("HTTP/1.1 200 OK").
pub(crate) fn parse_status(line: &str) -> Option<(u16, String)> {
    let mut it = line.splitn(3, ' ');
    let proto = it.next()?;
    if !proto.starts_with("HTTP/") {
        return None;
    }
    let code = it.next()?.trim().parse().ok()?;
    Some((code, it.next().unwrap_or("").trim().to_string()))
}

#[cfg(test)]
mod tests;
