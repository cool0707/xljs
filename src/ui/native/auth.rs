//! Basic authentication in the WebView2 windows (the app windows and the
//! browser pane of the development mode) with the logins of fetch
//! (net::auth): a proxy or a site that asks for Basic gets the login saved
//! for its realm; without one (or when it was refused) the user is asked
//! and the answer saved. WebView2 does not tell a proxy from a site, nor
//! whether the login worked: a second challenge for the realm soon after
//! means it was refused. Without a proxy in the settings of Windows the
//! challenge is the site's, and its login is kept for its host only; with
//! one, a login saved for the host comes first, then the proxy's.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, PWSTR};

use super::hstr;
use crate::net::auth::{self, Ask, Key, Login, LoginPrompt};

thread_local! {
    /// The challenges being answered (the arguments and their deferral).
    static PENDING: RefCell<HashMap<u64, (ICoreWebView2BasicAuthenticationRequestedEventArgs, ICoreWebView2Deferral)>> =
        RefCell::new(HashMap::new());
    /// The login last given for each key, and when.
    static GIVEN: RefCell<HashMap<Key, (Login, Instant)>> = RefCell::new(HashMap::new());
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A challenge again within this time means the login was refused.
const REFUSED_WITHIN: Duration = Duration::from_secs(30);

fn take(p: PWSTR) -> String {
    super::webview::take_pwstr(p)
}

/// Answers the Basic challenges of core (WebView2 1.0.1245+).
pub(super) fn attach(core: &ICoreWebView2) {
    let Ok(c10) = core.cast::<ICoreWebView2_10>() else { return };
    let mut token = 0i64;
    let _ = unsafe {
        c10.add_BasicAuthenticationRequested(
            &BasicAuthenticationRequestedEventHandler::create(Box::new(|_, args: Option<ICoreWebView2BasicAuthenticationRequestedEventArgs>| {
                if let Some(a) = args {
                    requested(a);
                }
                Ok(())
            })),
            &mut token,
        )
    };
}

fn requested(args: ICoreWebView2BasicAuthenticationRequestedEventArgs) {
    if !super::thread::can_post() {
        return;
    }
    let mut p = PWSTR::null();
    let challenge = if unsafe { args.Challenge(&mut p) }.is_ok() { take(p) } else { return };
    // Other schemes: WebView2's own handling.
    let Some(realm) = auth::basic_realm(&challenge) else { return };
    let mut p = PWSTR::null();
    let uri = if unsafe { args.Uri(&mut p) }.is_ok() { take(p) } else { String::new() };
    let host = uri.split("://").nth(1).and_then(|r| r.split(['/', '?', '#']).next()).unwrap_or("").to_string();
    let site = Key::site(&host, &realm);
    let key = if !host.is_empty() && (site.saved() || !crate::net::system_proxy()) { site } else { Key::proxy(&realm) };
    let failed = GIVEN.with(|g| g.borrow().get(&key).filter(|(_, at)| at.elapsed() < REFUSED_WITHIN).map(|(l, _)| l.clone()));
    let Ok(deferral) = (unsafe { args.GetDeferral() }) else { return };
    let id = NEXT.fetch_add(1, Ordering::SeqCst);
    PENDING.with(|m| m.borrow_mut().insert(id, (args, deferral)));
    // Not on the UI thread: auth::login waits for other prompts, which
    // run there.
    std::thread::spawn(move || {
        let ask: Ask = Arc::new(|p: &LoginPrompt| {
            let p = p.clone();
            super::thread::run_on_ui(move || {
                let (title, message) = p.texts();
                super::dialogs::credentials(&title, &message, &p.user, p.failed)
            })
        });
        let prompt = LoginPrompt { proxy: false, host, ..Default::default() };
        let l = auth::login(&key, prompt, failed.as_ref(), &ask);
        if let Some(l) = &l {
            auth::confirm(&key, l);
        }
        super::thread::post_ui(Box::new(move || complete(id, key, l)));
    });
}

fn complete(id: u64, key: Key, l: Option<Login>) {
    let Some((args, deferral)) = PENDING.with(|m| m.borrow_mut().remove(&id)) else { return };
    match l {
        Some(l) => {
            if let Ok(r) = unsafe { args.Response() } {
                let _ = unsafe { r.SetUserName(&hstr(&l.user)) };
                let _ = unsafe { r.SetPassword(&hstr(&l.password)) };
            }
            GIVEN.with(|g| g.borrow_mut().insert(key, (l, Instant::now())));
        }
        None => {
            let _ = unsafe { args.SetCancel(true) };
        }
    }
    let _ = unsafe { deferral.Complete() };
}
