//! Natives of shell.run / shell.runSync / shell.spawn (see crate::process).

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use rquickjs::{Ctx, Function, Object, Value};

use super::js::{self, nullish};
use super::{A, N, R};
use crate::engine::watchdog;
use crate::process::{Done, OnLines, Running, Spec, Stream, decode_output};
use crate::Error;

fn spec(a: &A) -> rquickjs::Result<Spec> {
    serde_json::from_str(&a.str(0)).map_err(|e| a.throw(&Error::from(e)))
}

/// The bytes for the standard input: a string in the encoding (UTF-8 for
/// "auto"), or bytes as they are.
fn input<'js>(a: &A<'js>, i: usize, enc: &str) -> rquickjs::Result<Option<Vec<u8>>> {
    let v = a.arg(i);
    if nullish(&v) {
        return Ok(None);
    }
    if v.is_string() {
        let enc = match enc {
            "" | "auto" | "buffer" => "utf8",
            e => e,
        };
        return crate::api::encode(&a.str(i), enc).map(Some).map_err(|e| a.throw(&e));
    }
    js::bytes(&a.ctx, &v).map(Some)
}

fn output<'js>(ctx: &Ctx<'js>, b: &[u8], enc: &str) -> R<'js> {
    if enc == "buffer" {
        js::array_buffer(ctx, b.to_vec())
    } else {
        Ok(js::v(ctx, decode_output(b, enc)))
    }
}

/// {exitCode, stdout, stderr, timedOut, pid}
fn result<'js>(ctx: &Ctx<'js>, d: &Done, enc: &str, pid: u32) -> R<'js> {
    let code = match d.code {
        Some(c) => js::v(ctx, c),
        None => Value::new_null(ctx.clone()),
    };
    js::object(
        ctx,
        vec![
            ("exitCode", code),
            ("stdout", output(ctx, &d.stdout, enc)?),
            ("stderr", output(ctx, &d.stderr, enc)?),
            ("timedOut", js::v(ctx, d.timed_out)),
            ("pid", js::v(ctx, pid)),
        ],
    )
}

fn func<'js>(v: Value<'js>) -> Option<Function<'js>> {
    v.as_function().cloned()
}

pub(super) fn install<'js>(n: &N<'js>) {
    // procRunSync(specJSON, input, onStdout, onStderr) -> result: waits on
    // the script thread; the callbacks are called here as lines arrive.
    n.set("procRunSync", |h, a| {
        let spec = spec(a)?;
        let input = input(a, 1, &spec.encoding)?;
        let (on_out, on_err) = (func(a.arg(2)), func(a.arg(3)));
        let want = on_out.is_some() || on_err.is_some();
        // The script runs in the debugger page, which cannot be called back
        // while it waits for this call: the lines go with the result
        // ("lines": [[0 = stdout | 1 = stderr, line], ...]).
        let in_debugger = h.ext.borrow().is_some();
        let (tx, rx) = mpsc::channel::<(Stream, Vec<String>)>();
        let cb: Option<OnLines> = if want {
            let tx = std::sync::Mutex::new(tx);
            Some(Arc::new(move |s, lines| {
                let _ = tx.lock().unwrap().send((s, lines));
            }))
        } else {
            drop(tx);
            None
        };
        let r = Running::start(&spec, input, cb).map_err(|e| a.throw(&e))?;
        let mut later: Vec<(Stream, String)> = Vec::new();
        let done = if !want {
            r.wait()
        } else {
            loop {
                match rx.recv_timeout(Duration::from_millis(20)) {
                    Ok((s, lines)) if in_debugger => later.extend(lines.into_iter().map(|l| (s, l))),
                    Ok((s, lines)) => {
                        watchdog::waited();
                        if let Some(f) = if s == Stream::Stdout { &on_out } else { &on_err } {
                            for l in lines {
                                f.call::<_, Value>((l,))?;
                            }
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(d) = r.done() {
                            break d;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break r.wait(),
                }
            }
        };
        watchdog::waited();
        let res = result(&a.ctx, &done, &spec.encoding, r.pid)?;
        if in_debugger && want {
            let ctx = &a.ctx;
            let items = later.into_iter().map(|(s, l)| js::array(ctx, [js::v(ctx, (s == Stream::Stderr) as i32), js::v(ctx, l)]));
            let items: Vec<Value> = items.collect::<rquickjs::Result<_>>()?;
            if let Some(o) = res.as_object() {
                o.set("lines", js::array(ctx, items)?)?;
            }
        }
        Ok(res)
    });

    // procSpawn(specJSON, input, onStdout, onStderr) -> {pid, wait, kill,
    // write, end, running}. Only wait() keeps the script running; the
    // callbacks are called while it runs.
    let weak = std::rc::Rc::downgrade(&n.host);
    n.set("procSpawn", move |h, a| {
        let spec = spec(a)?;
        let input = input(a, 1, &spec.encoding)?;
        let ctx = a.ctx.clone();
        let ids: Vec<Option<u64>> = [2, 3].iter().map(|&i| func(a.arg(i)).map(|f| h.keep_fn(&ctx, f))).collect();
        let (out_id, err_id) = (ids[0], ids[1]);
        let cb: Option<OnLines> = if out_id.is_some() || err_id.is_some() {
            let poster = h.poster();
            Some(Arc::new(move |s, lines: Vec<String>| {
                let Some(id) = (if s == Stream::Stdout { out_id } else { err_id }) else { return };
                poster.add_pending();
                poster.post(Box::new(move |ctx, host| {
                    if let Some(f) = host.kept_fn(ctx, id) {
                        for l in lines {
                            f.call::<_, Value>((l,))?;
                        }
                    }
                    Ok(())
                }));
            }))
        } else {
            None
        };
        let r = match Running::start(&spec, input, cb) {
            Ok(r) => r,
            Err(e) => {
                for id in ids.into_iter().flatten() {
                    h.drop_fn(id);
                }
                return Err(a.throw(&e));
            }
        };
        let obj = Object::new(ctx.clone())?;
        obj.set("pid", r.pid)?;
        let (r1, enc, hw) = (r.clone(), spec.encoding.clone(), weak.clone());
        obj.set(
            "wait",
            Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> R<'js> {
                let (promise, resolve, _) = ctx.promise()?;
                let Some(h) = hw.upgrade() else { return Ok(promise.into_value()) };
                let id = h.keep_fn(&ctx, resolve);
                let poster = h.poster();
                poster.add_pending();
                let (enc, pid) = (enc.clone(), r1.pid);
                r1.when_done(move |d| {
                    let d = d.clone();
                    poster.post(Box::new(move |ctx, host| {
                        if let Some(f) = host.kept_fn(ctx, id) {
                            host.drop_fn(id);
                            f.call::<_, Value>((result(ctx, &d, &enc, pid)?,))?;
                        }
                        for id in [out_id, err_id].into_iter().flatten() {
                            host.drop_fn(id);
                        }
                        Ok(())
                    }));
                });
                Ok(promise.into_value())
            })?,
        )?;
        let r2 = r.clone();
        obj.set("kill", Function::new(ctx.clone(), move || r2.kill())?)?;
        let r3 = r.clone();
        obj.set("running", Function::new(ctx.clone(), move || r3.running())?)?;
        let r4 = r.clone();
        obj.set("end", Function::new(ctx.clone(), move || r4.end())?)?;
        let (r5, enc) = (r.clone(), spec.encoding.clone());
        obj.set(
            "write",
            Function::new(ctx.clone(), move |ctx: Ctx<'js>, v: Value<'js>| -> rquickjs::Result<()> {
                let b = if v.is_string() {
                    let e = match enc.as_str() {
                        "" | "auto" | "buffer" => "utf8",
                        e => e,
                    };
                    crate::api::encode(&js::to_str(&v), e).map_err(|e| super::throw(&ctx, &e))?
                } else {
                    js::bytes(&ctx, &v)?
                };
                r5.write(&b).map_err(|e| super::throw(&ctx, &e))
            })?,
        )?;
        Ok(obj.into_value())
    });
}
