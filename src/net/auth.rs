//! User names and passwords for Basic authentication, shared by fetch
//! (proxies that answer 407 with Basic) and the WebView2 windows (their
//! BasicAuthenticationRequested event). The first time the user is asked
//! (Ui::credentials); the credentials that worked are kept by the realm of
//! the challenge in the Windows Credential Manager (generic credentials
//! "xljs:Basic:<realm>", persisted for the user on this computer), in
//! memory elsewhere and in the tests.

use std::sync::{Arc, Mutex};

/// A user name and a password.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Login {
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for Login {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Login({:?}, ***)", self.user)
    }
}

/// What the user is asked for.
#[derive(Debug, Clone, Default)]
pub struct LoginPrompt {
    /// For a proxy (fetch); false for a WebView2 window, which cannot tell
    /// a proxy from the site.
    pub proxy: bool,
    /// The proxy or the site ("" when unknown).
    pub host: String,
    /// The realm of the challenge.
    pub realm: String,
    /// The user name to show (the one that failed).
    pub user: String,
    /// Whether the last attempt was refused.
    pub failed: bool,
}

impl LoginPrompt {
    /// The title and the message of the prompt.
    pub fn texts(&self) -> (String, String) {
        let mut m = match (self.proxy, self.host.is_empty()) {
            (true, true) => crate::t!("net.loginProxy"),
            (true, false) => crate::t!("net.loginProxyHost", self.host),
            (false, _) => crate::t!("net.loginSite", self.host),
        };
        if !self.realm.is_empty() {
            m += &crate::t!("net.loginRealm", self.realm);
        }
        if self.failed {
            m = crate::t!("net.loginFailed") + "\n" + &m;
        }
        m += "\n";
        m += &crate::t!("net.loginSaved");
        (crate::t!("net.loginTitle"), m)
    }
}

/// Asks the user (Ui::credentials); None when cancelled.
pub type Ask = Arc<dyn Fn(&LoginPrompt) -> Option<Login> + Send + Sync>;

/// The realm of a challenge ("Basic realm=\"x\"") when it is Basic.
pub fn basic_realm(challenge: &str) -> Option<String> {
    let c = challenge.trim();
    let rest = c
        .get(..5)
        .filter(|s| s.eq_ignore_ascii_case("basic"))
        .map(|_| &c[5..])?;
    if !rest.is_empty() && !rest.starts_with([' ', '\t', ',']) {
        return None;
    }
    let lower = rest.to_ascii_lowercase();
    let Some(i) = lower.find("realm=") else {
        return Some(String::new());
    };
    let v = &rest[i + 6..];
    Some(match v.strip_prefix('"') {
        Some(q) => q.split('"').next().unwrap_or("").to_string(),
        None => v.split([',', ' ']).next().unwrap_or("").to_string(),
    })
}

/// One prompt at a time: requests that meet the same challenge together
/// ask once and use what the first one saved.
static PROMPT: Mutex<()> = Mutex::new(());

/// The login for a Basic challenge of realm: the saved one, or what the
/// user enters (saved only by confirm). failed is the login that was just
/// refused: it is forgotten, and a different saved one (saved meanwhile by
/// another request) is used without asking. Must not be called on the UI
/// thread (it waits for the prompt there).
pub fn login(realm: &str, prompt: LoginPrompt, failed: Option<&Login>, ask: &Ask) -> Option<Login> {
    let _g = PROMPT.lock().unwrap_or_else(|e| e.into_inner());
    let saved = store::get(realm);
    match (&saved, failed) {
        (Some(s), None) => return Some(s.clone()),
        (Some(s), Some(f)) if s != f => return Some(s.clone()),
        (Some(_), Some(_)) => store::delete(realm),
        _ => {}
    }
    let p = LoginPrompt {
        realm: realm.to_string(),
        user: failed.map(|f| f.user.clone()).unwrap_or_default(),
        failed: failed.is_some(),
        ..prompt
    };
    ask(&p).filter(|l| !l.user.is_empty())
}

/// Keeps a login that worked.
pub fn confirm(realm: &str, l: &Login) {
    if store::get(realm).as_ref() != Some(l) {
        store::put(realm, l);
    }
}

/// The name of the generic credential in the Credential Manager.
#[cfg_attr(any(test, not(windows)), allow(dead_code))]
fn target(realm: &str) -> String {
    format!("xljs:Basic:{realm}")
}

#[cfg(any(test, not(windows)))]
mod store {
    use super::Login;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static MEM: Mutex<Option<HashMap<String, Login>>> = Mutex::new(None);

    pub fn get(realm: &str) -> Option<Login> {
        MEM.lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(realm).cloned())
    }
    pub fn put(realm: &str, l: &Login) {
        MEM.lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(realm.to_string(), l.clone());
    }
    pub fn delete(realm: &str) {
        if let Some(m) = MEM.lock().unwrap().as_mut() {
            m.remove(realm);
        }
    }
    #[cfg(test)]
    pub fn clear() {
        *MEM.lock().unwrap() = None;
    }
}

#[cfg(all(windows, not(test)))]
mod store {
    use super::{Login, target};
    use windows::Win32::Security::Credentials::*;
    use windows::core::{HSTRING, PWSTR};

    pub fn get(realm: &str) -> Option<Login> {
        let mut p: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(
                &HSTRING::from(target(realm)),
                CRED_TYPE_GENERIC,
                None,
                &mut p,
            )
        }
        .ok()?;
        let l = unsafe {
            let c = &*p;
            let user = if c.UserName.is_null() {
                String::new()
            } else {
                c.UserName.to_string().unwrap_or_default()
            };
            let blob = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let w: Vec<u16> = blob
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            CredFree(p as *const _);
            Login {
                user,
                password: String::from_utf16_lossy(&w),
            }
        };
        Some(l)
    }

    pub fn put(realm: &str, l: &Login) {
        let mut name: Vec<u16> = target(realm).encode_utf16().chain([0]).collect();
        let mut user: Vec<u16> = l.user.encode_utf16().chain([0]).collect();
        let mut comment: Vec<u16> = "xljs".encode_utf16().chain([0]).collect();
        let mut blob: Vec<u8> = l
            .password
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        let c = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(name.as_mut_ptr()),
            Comment: PWSTR(comment.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(user.as_mut_ptr()),
            ..Default::default()
        };
        let _ = unsafe { CredWriteW(&c, 0) };
        blob.fill(0);
    }

    pub fn delete(realm: &str) {
        let _ = unsafe { CredDeleteW(&HSTRING::from(target(realm)), CRED_TYPE_GENERIC, None) };
    }
}

/// Forgets the logins kept in memory (between the script tests).
#[cfg(test)]
pub(crate) fn clear() {
    store::clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realms() {
        assert_eq!(
            basic_realm(r#"Basic realm="社内 Proxy", charset="UTF-8""#).as_deref(),
            Some("社内 Proxy")
        );
        assert_eq!(basic_realm("basic realm=proxy").as_deref(), Some("proxy"));
        assert_eq!(basic_realm("Basic").as_deref(), Some(""));
        assert_eq!(basic_realm("Negotiate"), None);
        assert_eq!(basic_realm("Basicx realm=a"), None);
    }
}
