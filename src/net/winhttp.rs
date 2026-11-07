//! Requests through WinHTTP (synchronous, on the worker thread of the
//! request). One session for the process: the proxy of the system
//! (WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PAC included), keep-alive
//! connections and cookies are shared by the requests of a run. Redirects
//! are left to the prelude; 401 and 407 with Negotiate or NTLM are answered
//! with the signed-in user (by the autologon policy: intranet servers by
//! default, every server with credentials "include").

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Win32::Networking::WinHttp::*;
use windows::core::{HSTRING, PCWSTR, PWSTR};

use super::{CONNECT_TIMEOUT, Cancel, Credentials, IO_TIMEOUT, Request, Response, Target, parse_headers, parse_status};
use crate::{Error, Result, err};

/// The handle of the session (0: WinHttpOpen failed).
static SESSION: OnceLock<usize> = OnceLock::new();

fn session(agent: &str) -> Option<*mut c_void> {
    let h = *SESSION.get_or_init(|| unsafe {
        let a = HSTRING::from(agent);
        let mut h = WinHttpOpen(&a, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if h.is_null() {
            // Before Windows 8.1.
            h = WinHttpOpen(&a, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        }
        h as usize
    });
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
        FormatMessageW(flags, module.map(|m| m.0 as *const c_void), c, 0, PWSTR(buf.as_mut_ptr()), buf.len() as u32, None)
    };
    let s = String::from_utf16_lossy(&buf[..n as usize]).trim().to_string();
    if s.is_empty() { format!("WinHTTP {c}") } else { format!("{s} (WinHTTP {c})") }
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

pub(super) fn send(req: &Request, t: &Target, cancel: &Cancel) -> Result<Response> {
    let s = session(&req.agent).ok_or_else(|| error(req, last_code()))?;
    let conn = H(unsafe { WinHttpConnect(s, &HSTRING::from(t.host.as_str()), t.port, 0) });
    if conn.0.is_null() {
        return Err(error(req, last_code()));
    }
    let r = send_on(req, t, cancel, conn.0);
    // The request handle goes before the connection's.
    cancel.finish();
    drop(conn);
    r
}

fn send_on(req: &Request, t: &Target, cancel: &Cancel, conn: *mut c_void) -> Result<Response> {
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
    set_u32(h, WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER);
    // gzip and deflate (WINHTTP_DECOMPRESSION_FLAG_ALL).
    set_u32(h, WINHTTP_OPTION_DECOMPRESSION, 3);
    match req.credentials {
        Credentials::Omit => {
            set_u32(h, WINHTTP_OPTION_AUTOLOGON_POLICY, WINHTTP_AUTOLOGON_SECURITY_LEVEL_HIGH);
            set_u32(h, WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_DISABLE_COOKIES);
        }
        Credentials::Include => set_u32(h, WINHTTP_OPTION_AUTOLOGON_POLICY, WINHTTP_AUTOLOGON_SECURITY_LEVEL_LOW),
        Credentials::SameOrigin => {}
    }
    let mut has_auth = false;
    for (k, v) in &req.headers {
        has_auth |= k.eq_ignore_ascii_case("authorization");
        let line = wide(&format!("{k}: {v}"));
        unsafe { WinHttpAddRequestHeaders(h, &line, WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE) }
            .map_err(|e| error(req, code(&e)))?;
    }

    let body = req.body.as_deref().unwrap_or(&[]);
    let mut tries = 0;
    let status = loop {
        let (ptr, len) = if body.is_empty() { (None, 0) } else { (Some(body.as_ptr() as *const c_void), body.len() as u32) };
        unsafe { WinHttpSendRequest(h, None, ptr, len, len, 0) }.map_err(|e| error(req, code(&e)))?;
        unsafe { WinHttpReceiveResponse(h, std::ptr::null_mut()) }.map_err(|e| error(req, code(&e)))?;
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
        let auth = status == 407 || (status == 401 && !has_auth);
        if !auth || req.credentials == Credentials::Omit || tries >= 3 {
            break status;
        }
        let (mut supported, mut first, mut target) = (0u32, 0u32, 0u32);
        if unsafe { WinHttpQueryAuthSchemes(h, &mut supported, &mut first, &mut target) }.is_err() {
            break status;
        }
        let scheme = if supported & WINHTTP_AUTH_SCHEME_NEGOTIATE.0 != 0 {
            WINHTTP_AUTH_SCHEME_NEGOTIATE.0
        } else if supported & WINHTTP_AUTH_SCHEME_NTLM.0 != 0 {
            WINHTTP_AUTH_SCHEME_NTLM.0
        } else {
            break status;
        };
        // No user name: the signed-in user.
        if unsafe { WinHttpSetCredentials(h, target, scheme, PCWSTR::null(), PCWSTR::null(), std::ptr::null_mut()) }.is_err() {
            break status;
        }
        tries += 1;
    };

    let mut size = 0u32;
    let _ = unsafe { WinHttpQueryHeaders(h, WINHTTP_QUERY_RAW_HEADERS_CRLF, PCWSTR::null(), None, &mut size, std::ptr::null_mut()) };
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
    let status_text = lines.next().and_then(parse_status).map(|(_, t)| t).unwrap_or_default();
    let headers = parse_headers(lines);

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
    Ok(Response { status: status as u16, status_text, headers, body: out })
}
