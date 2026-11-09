//! Requests through WinHTTP (synchronous, on the worker thread of the
//! request). One session for the process: the proxy of the system
//! (WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PAC included), keep-alive
//! connections and cookies are shared by the requests of a run. Redirects
//! are left to the prelude; 401 and 407 with Negotiate or NTLM are answered
//! with the signed-in user (by the autologon policy: intranet servers by
//! default, every server with credentials "include").

use std::ffi::c_void;
use std::sync::Mutex;

use windows::Win32::Networking::WinHttp::*;
use windows::core::{HSTRING, PCWSTR, PWSTR};

use super::auth::{self, Login, LoginPrompt};
use super::{
    CONNECT_TIMEOUT, Cancel, Credentials, IO_TIMEOUT, Request, Response, Target, parse_headers,
    parse_status,
};
use crate::{Error, Result, err};

/// The sessions by proxy ("": the system's), as handles (0: WinHttpOpen
/// failed).
static SESSIONS: Mutex<Vec<(String, usize)>> = Mutex::new(Vec::new());

fn session(agent: &str, proxy: Option<&str>) -> Option<*mut c_void> {
    let key = proxy.unwrap_or("").to_string();
    let mut all = SESSIONS.lock().unwrap();
    let h = match all.iter().find(|(k, _)| *k == key) {
        Some((_, h)) => *h,
        None => {
            let h = unsafe {
                let a = HSTRING::from(agent);
                match proxy {
                    // <-loopback>: also for this computer (the tests).
                    Some(p) => WinHttpOpen(
                        &a,
                        WINHTTP_ACCESS_TYPE_NAMED_PROXY,
                        &HSTRING::from(p),
                        &HSTRING::from("<-loopback>"),
                        0,
                    ),
                    None => {
                        let mut h = WinHttpOpen(
                            &a,
                            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                            PCWSTR::null(),
                            PCWSTR::null(),
                            0,
                        );
                        if h.is_null() {
                            // Before Windows 8.1.
                            h = WinHttpOpen(
                                &a,
                                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                                PCWSTR::null(),
                                PCWSTR::null(),
                                0,
                            );
                        }
                        h
                    }
                }
            } as usize;
            all.push((key, h));
            h
        }
    };
    (h != 0).then_some(h as *mut c_void)
}

/// The WinHTTP error code (12xxx) of an error.
fn code(e: &windows::core::Error) -> u32 {
    let h = e.code().0 as u32;
    if h >> 16 == 0x8007 { h & 0xffff } else { h }
}

fn last_code() -> u32 {
    code(&windows::core::Error::from_thread())
}

/// The text of the system for a WinHTTP error code.
fn message(c: u32) -> String {
    use windows::Win32::System::Diagnostics::Debug::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    let mut buf = [0u16; 512];
    let module = unsafe { GetModuleHandleW(&HSTRING::from("winhttp.dll")) }.ok();
    let mut flags = FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS;
    if module.is_some() {
        flags |= FORMAT_MESSAGE_FROM_HMODULE;
    }
    let n = unsafe {
        FormatMessageW(
            flags,
            module.map(|m| m.0 as *const c_void),
            c,
            0,
            PWSTR(buf.as_mut_ptr()),
            buf.len() as u32,
            None,
        )
    };
    let s = String::from_utf16_lossy(&buf[..n as usize])
        .trim()
        .to_string();
    if s.is_empty() {
        format!("WinHTTP {c}")
    } else {
        format!("{s} (WinHTTP {c})")
    }
}

fn error(req: &Request, c: u32) -> Error {
    match c {
        ERROR_WINHTTP_TIMEOUT => err!("net.timeout", req.url),
        ERROR_WINHTTP_NAME_NOT_RESOLVED => err!("net.unresolved", req.url),
        ERROR_WINHTTP_CANNOT_CONNECT => err!("net.cannotConnect", req.url, message(c)),
        // Certificate errors.
        ERROR_WINHTTP_SECURE_FAILURE | 12037 | 12038 | 12044 | 12045 | 12169 | 12179 => {
            err!("net.secureFailure", req.url, message(c))
        }
        _ => err!("net.failed", req.url, message(c)),
    }
}

/// A handle closed when dropped.
struct H(*mut c_void);

impl Drop for H {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn set_u32(h: *mut c_void, option: u32, v: u32) {
    unsafe {
        let _ = WinHttpSetOption(Some(h as *const c_void), option, Some(&v.to_ne_bytes()));
    }
}

/// The status text and the headers of the response.
fn raw_headers(req: &Request, h: *mut c_void) -> Result<(String, Vec<(String, String)>)> {
    let mut size = 0u32;
    let _ = unsafe {
        WinHttpQueryHeaders(
            h,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            PCWSTR::null(),
            None,
            &mut size,
            std::ptr::null_mut(),
        )
    };
    let mut raw = vec![0u16; (size as usize).div_ceil(2) + 1];
    let mut size = (raw.len() * 2) as u32;
    unsafe {
        WinHttpQueryHeaders(
            h,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            PCWSTR::null(),
            Some(raw.as_mut_ptr() as *mut c_void),
            &mut size,
            std::ptr::null_mut(),
        )
    }
    .map_err(|e| error(req, code(&e)))?;
    let raw = String::from_utf16_lossy(&raw[..(size as usize / 2).min(raw.len())]);
    let mut lines = raw.split("\r\n");
    let status_text = lines
        .next()
        .and_then(parse_status)
        .map(|(_, t)| t)
        .unwrap_or_default();
    Ok((status_text, parse_headers(lines)))
}

pub(super) fn send(req: &Request, t: &Target, cancel: &Cancel) -> Result<Response> {
    let proxy = super::proxy_override();
    let s = session(&req.agent, proxy.as_deref()).ok_or_else(|| error(req, last_code()))?;
    let conn = H(unsafe { WinHttpConnect(s, &HSTRING::from(t.host.as_str()), t.port, 0) });
    if conn.0.is_null() {
        return Err(error(req, last_code()));
    }
    // A new request handle for each login of a Basic proxy: WinHTTP keeps
    // using the first one given to a handle.
    let mut st = ProxyBasic::default();
    let r = loop {
        let r = send_on(
            req,
            t,
            cancel,
            conn.0,
            proxy.as_deref().unwrap_or(""),
            &mut st,
        );
        match r {
            Ok(None) if !cancel.cancelled() => {
                // Closes this handle; the next one registers its own.
                cancel.finish();
            }
            Ok(None) => break Err(err!("net.failed", req.url, "aborted")),
            Ok(Some(res)) => break Ok(res),
            Err(e) => break Err(e),
        }
    };
    // The request handle goes before the connection's.
    cancel.finish();
    drop(conn);
    r
}

/// The Basic login of a proxy across request handles.
#[derive(Default)]
struct ProxyBasic {
    /// The login to give at the first 407 (and its realm).
    login: Option<(String, Login)>,
    /// Retries with login.
    prompts: u32,
}

/// One request handle; None to send again on a new handle
/// with st.login.
fn send_on(
    req: &Request,
    t: &Target,
    cancel: &Cancel,
    conn: *mut c_void,
    proxy: &str,
    st: &mut ProxyBasic,
) -> Result<Option<Response>> {
    let mut flags = WINHTTP_FLAG_ESCAPE_DISABLE | WINHTTP_FLAG_ESCAPE_DISABLE_QUERY;
    if t.secure {
        flags |= WINHTTP_FLAG_SECURE;
    }
    let h = unsafe {
        WinHttpOpenRequest(
            conn,
            &HSTRING::from(req.method.as_str()),
            &HSTRING::from(t.path.as_str()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    };
    if h.is_null() {
        return Err(error(req, last_code()));
    }
    let hv = h as usize;
    // Closing the handle interrupts a call that waits on it.
    if !cancel.set_closer(move || unsafe {
        let _ = WinHttpCloseHandle(hv as *mut c_void);
    }) {
        return Err(err!("net.failed", req.url, "aborted"));
    }

    let ms = |d: std::time::Duration| d.as_millis().min(i32::MAX as u128) as i32;
    let io = ms(req.timeout.unwrap_or(IO_TIMEOUT));
    unsafe {
        let _ = WinHttpSetTimeouts(h, 0, ms(CONNECT_TIMEOUT), io, io);
    }
    set_u32(
        h,
        WINHTTP_OPTION_REDIRECT_POLICY,
        WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    );
    // gzip and deflate (WINHTTP_DECOMPRESSION_FLAG_ALL).
    set_u32(h, WINHTTP_OPTION_DECOMPRESSION, 3);
    // The autologon policy is for the servers (credentials); proxies get
    // the signed-in user anyway (see below).
    let policy = match req.credentials {
        Credentials::Omit => WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH,
        Credentials::Include => WINHTTP_AUTOLOGON_SECURITY_LEVEL_LOW,
        Credentials::SameOrigin => WINHTTP_AUTOLOGON_SECURITY_LEVEL_MEDIUM,
    };
    set_u32(h, WINHTTP_OPTION_AUTOLOGON_POLICY, policy);
    if req.credentials == Credentials::Omit {
        set_u32(h, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_DISABLE_COOKIES);
    }
    let mut has_auth = false;
    for (k, v) in &req.headers {
        has_auth |= k.eq_ignore_ascii_case("authorization");
        let line = wide(&format!("{k}: {v}"));
        unsafe {
            WinHttpAddRequestHeaders(
                h,
                &line,
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            )
        }
        .map_err(|e| error(req, code(&e)))?;
    }
    // The session keeps the agent of its first request: each request
    // sends its own.
    if !req
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
    {
        let line = wide(&format!("User-Agent: {}", req.agent));
        let _ = unsafe {
            WinHttpAddRequestHeaders(
                h,
                &line,
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            )
        };
    }

    let body = req.body.as_deref().unwrap_or(&[]);
    // Answers to 401 (Windows authentication) and 407: the proxy's
    // Negotiate / NTLM with the signed-in user, then Basic with the saved
    // or entered login (prompts count separately: the user may retry).
    let (mut server_tries, mut proxy_tries) = (0, 0);
    // The login given to this handle.
    let mut basic: Option<(String, Login)> = None;
    let (status, status_text, headers) = loop {
        let (ptr, len) = if body.is_empty() {
            (None, 0)
        } else {
            (Some(body.as_ptr() as *const c_void), body.len() as u32)
        };
        unsafe { WinHttpSendRequest(h, None, ptr, len, len, 0) }
            .map_err(|e| error(req, code(&e)))?;
        unsafe { WinHttpReceiveResponse(h, std::ptr::null_mut()) }
            .map_err(|e| error(req, code(&e)))?;
        let mut status = 0u32;
        let mut size = 4u32;
        unsafe {
            WinHttpQueryHeaders(
                h,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some(&mut status as *mut u32 as *mut c_void),
                &mut size,
                std::ptr::null_mut(),
            )
        }
        .map_err(|e| error(req, code(&e)))?;
        let (status_text, headers) = raw_headers(req, h)?;
        if status != 407
            && let Some((realm, l)) = basic.take()
        {
            auth::confirm(&realm, &l);
        }
        if status == 407 {
            let (mut supported, mut first, mut target) = (0u32, 0u32, 0u32);
            let _ = unsafe { WinHttpQueryAuthSchemes(h, &mut supported, &mut first, &mut target) };
            let integrated = [WINHTTP_AUTH_SCHEME_NEGOTIATE.0, WINHTTP_AUTH_SCHEME_NTLM.0]
                .into_iter()
                .find(|s| supported & s != 0);
            if let Some(scheme) = integrated
                && proxy_tries < 2
            {
                // The signed-in user whatever the autologon policy says
                // (it would keep it from proxies outside the intranet).
                set_u32(
                    h,
                    WINHTTP_OPTION_AUTOLOGON_POLICY,
                    WINHTTP_AUTOLOGON_SECURITY_LEVEL_LOW,
                );
                let ok = unsafe {
                    WinHttpSetCredentials(
                        h,
                        WINHTTP_AUTH_TARGET_PROXY,
                        scheme,
                        PCWSTR::null(),
                        PCWSTR::null(),
                        std::ptr::null_mut(),
                    )
                }
                .is_ok();
                if ok {
                    proxy_tries += 1;
                    continue;
                }
            }
            if supported & WINHTTP_AUTH_SCHEME_BASIC.0 != 0
                && st.prompts < 5
                && let Some(ask) = &req.ask
            {
                let realm = headers
                    .iter()
                    .filter(|(k, _)| k.eq_ignore_ascii_case("proxy-authenticate"))
                    .find_map(|(_, v)| auth::basic_realm(v))
                    .unwrap_or_default();
                let preset = if basic.is_none() {
                    st.login.take().filter(|(r, _)| *r == realm)
                } else {
                    None
                };
                let next = match preset {
                    Some((_, l)) => Some(l),
                    None => {
                        let failed = basic.take().map(|(_, l)| l);
                        let prompt = LoginPrompt {
                            proxy: true,
                            host: proxy.to_string(),
                            ..Default::default()
                        };
                        if failed.is_some() {
                            st.prompts += 1;
                        }
                        let l = auth::login(&realm, prompt, failed.as_ref(), ask);
                        if failed.is_some()
                            && let Some(l) = l
                        {
                            // This handle would send the refused one again.
                            st.login = Some((realm, l));
                            return Ok(None);
                        }
                        l
                    }
                };
                if let Some(l) = next {
                    let ok = unsafe {
                        WinHttpSetCredentials(
                            h,
                            WINHTTP_AUTH_TARGET_PROXY,
                            WINHTTP_AUTH_SCHEME_BASIC.0,
                            &HSTRING::from(l.user.as_str()),
                            &HSTRING::from(l.password.as_str()),
                            std::ptr::null_mut(),
                        )
                    }
                    .is_ok();
                    if ok {
                        basic = Some((realm, l));
                        continue;
                    }
                }
            }
            break (status, status_text, headers);
        }
        // The servers get the autologon policy of credentials again.
        if proxy_tries > 0 {
            set_u32(h, WINHTTP_OPTION_AUTOLOGON_POLICY, policy);
        }
        if status != 401 || has_auth || req.credentials == Credentials::Omit || server_tries >= 3 {
            break (status, status_text, headers);
        }
        let (mut supported, mut first, mut target) = (0u32, 0u32, 0u32);
        if unsafe { WinHttpQueryAuthSchemes(h, &mut supported, &mut first, &mut target) }.is_err() {
            break (status, status_text, headers);
        }
        let scheme = if supported & WINHTTP_AUTH_SCHEME_NEGOTIATE.0 != 0 {
            WINHTTP_AUTH_SCHEME_NEGOTIATE.0
        } else if supported & WINHTTP_AUTH_SCHEME_NTLM.0 != 0 {
            WINHTTP_AUTH_SCHEME_NTLM.0
        } else {
            break (status, status_text, headers);
        };
        // No user name: the signed-in user.
        if unsafe {
            WinHttpSetCredentials(
                h,
                target,
                scheme,
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null_mut(),
            )
        }
        .is_err()
        {
            break (status, status_text, headers);
        }
        server_tries += 1;
    };

    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut n = 0u32;
        unsafe { WinHttpReadData(h, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut n) }
            .map_err(|e| error(req, code(&e)))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    if cancel.cancelled() {
        return Err(err!("net.failed", req.url, "aborted"));
    }
    Ok(Some(Response {
        status: status as u16,
        status_text,
        headers,
        body: out,
    }))
}
