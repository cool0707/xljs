//! Natives of fetch (see crate::net; the prelude builds fetch, Request,
//! Response and Headers on them).

use std::time::Duration;

use rquickjs::Value;

use super::N;
use super::js;
use crate::i18n;
use crate::net::{self, Cancel, Credentials};

pub(crate) fn install(n: &N) {
    // httpRequest(token, method, url, headersJSON, body | null, timeoutMs,
    // credentials) -> Promise<[status, statusText, headersJSON, ArrayBuffer]
    // | [0, message]>. One request without redirects, on a thread of its
    // own; httpAbort(token) interrupts it.
    n.set("httpRequest", |h, a| {
        let ctx = a.ctx.clone();
        let token = a.int(0) as u64;
        let headers: Vec<(String, String)> = serde_json::from_str(&a.str(3)).unwrap_or_default();
        let body = if js::nullish(&a.arg(4)) {
            None
        } else {
            Some(a.bytes(4)?)
        };
        let ms = a.f64(5);
        let req = net::Request {
            method: a.str(1),
            url: a.str(2),
            headers,
            body,
            timeout: (ms.is_finite() && ms > 0.0).then(|| Duration::from_millis(ms as u64)),
            credentials: Credentials::parse(&a.str(6)),
            agent: net::agent::for_fetch(&h.env.version),
            ask: Some({
                let ui = h.env.ui.clone();
                std::sync::Arc::new(move |p: &net::auth::LoginPrompt| ui.credentials(p))
            }),
        };
        let (promise, resolve, _) = ctx.promise()?;
        let id = h.keep_fn(&ctx, resolve);
        let cancel = Cancel::default();
        h.requests.borrow_mut().insert(token, cancel.clone());
        let poster = h.poster();
        poster.add_pending();
        std::thread::spawn(move || {
            let r = net::send(&req, &cancel);
            poster.post(Box::new(move |ctx, host| {
                host.requests.borrow_mut().remove(&token);
                let v = match r {
                    Ok(res) => js::array(
                        ctx,
                        [
                            js::v(ctx, res.status as f64),
                            js::v(ctx, res.status_text),
                            js::v(ctx, serde_json::to_string(&res.headers).unwrap_or_default()),
                            js::array_buffer(ctx, res.body)?,
                        ],
                    )?,
                    Err(e) => js::array(ctx, [js::v(ctx, 0.0), js::v(ctx, i18n::error(&e))])?,
                };
                if let Some(f) = host.kept_fn(ctx, id) {
                    f.call::<_, Value>((v,))?;
                }
                host.drop_fn(id);
                Ok(())
            }));
        });
        Ok(promise.into_value())
    });
    n.set("httpAbort", |h, a| {
        let c = h.requests.borrow_mut().remove(&(a.int(0) as u64));
        if let Some(c) = c {
            c.abort();
        }
        a.undefined()
    });
}
