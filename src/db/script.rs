//! SQL functions written in JavaScript (`db.function(name, fn)`).
//!
//! They exist only on the connections of the script that registered them:
//! other people's scripts, repair() and other tools do not know them. So
//! they are for the script's own statements only and must not get into
//! the schema, where a function that a connection does not know breaks the
//! table (generated columns, CHECK, indexes: `unknown function`, even for
//! SELECT * and INSERT) or the view and trigger:
//!
//! - they are registered with SQLITE_DIRECTONLY, so SQLite refuses them in
//!   generated columns, CHECK constraints and indexes (when they are
//!   created) and when a view or trigger calls them;
//! - views, triggers and DEFAULT values are not checked by SQLite when they
//!   are created, so CREATE and ALTER statements that call one of the
//!   functions are refused before they run (check_schema_sql);
//! - in shared mode with a change log, statements that change the database
//!   may not call them, because repair() replays the log without them.
//!
//! The connection calls back through the caller set with with_calls (the
//! natives of the running call), with the function's id.

use std::cell::Cell;
use std::ffi::CString;

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::Value;

use crate::{Result, err, t};

/// A function of the script.
#[derive(Clone, Debug)]
pub struct ScriptFn {
    pub name: String,
    /// What the caller knows the function by.
    pub id: u64,
    pub deterministic: bool,
}

/// Calls a function of the script: (id, arguments) → the value, or the
/// message of the error it threw.
pub type Call<'a> = dyn FnMut(u64, Vec<Value>) -> std::result::Result<Value, String> + 'a;

thread_local! {
    static CALL: Cell<Option<*mut Call<'static>>> = const { Cell::new(None) };
    /// Whether the statement running changes a database with a change log.
    static LOGGED: Cell<bool> = const { Cell::new(false) };
}

/// Runs f with call as the caller of the functions of the script.
pub fn with_calls<R>(call: &mut Call<'_>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<*mut Call<'static>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CALL.with(|c| c.set(self.0));
        }
    }
    let p: *mut Call<'_> = call;
    // The pointer is only used while f runs (Restore puts the previous one
    // back however f ends).
    let p: *mut Call<'static> = unsafe { std::mem::transmute(p) };
    let _restore = Restore(CALL.with(|c| c.replace(Some(p))));
    f()
}

/// Marks the statements run while the guard lives as changes that go into
/// a change log.
pub(crate) struct Logged(bool);

impl Logged {
    pub(crate) fn set(on: bool) -> Logged {
        Logged(LOGGED.with(|l| l.replace(on)))
    }
}

impl Drop for Logged {
    fn drop(&mut self) {
        LOGGED.with(|l| l.set(self.0));
    }
}

fn fail(msg: String) -> rusqlite::Error {
    rusqlite::Error::UserFunctionError(msg.into())
}

/// Registers a function of the script on a connection.
pub(crate) fn register(conn: &Connection, f: &ScriptFn) -> rusqlite::Result<()> {
    let mut flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DIRECTONLY;
    if f.deterministic {
        flags |= FunctionFlags::SQLITE_DETERMINISTIC;
    }
    let (id, name) = (f.id, f.name.clone());
    conn.create_scalar_function(f.name.as_str(), -1, flags, move |ctx| {
        if LOGGED.with(|l| l.get()) {
            return Err(fail(t!("sqlite.functionInLoggedWrite", name)));
        }
        let args = (0..ctx.len())
            .map(|i| ctx.get::<Value>(i))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Taken while it runs: a statement the function itself runs finds
        // no caller (the natives refuse such calls before that).
        let Some(call) = CALL.with(|c| c.take()) else {
            return Err(fail(t!("sqlite.functionUnavailable", name)));
        };
        let r = unsafe { (*call)(id, args) };
        CALL.with(|c| c.set(Some(call)));
        r.map_err(fail)
    })
}

/// Whether name can be the name of a function: letters, digits and _, not
/// starting with a digit.
pub(crate) fn valid_name(name: &str) -> bool {
    let mut cs = name.chars();
    cs.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && cs.all(|c| c.is_alphanumeric() || c == '_')
}

#[derive(Debug, PartialEq)]
enum Tok {
    /// A name (lower case), quoted or not.
    Name(String),
    Open,
    Semi,
    Other,
}

/// The tokens of sql with the byte offset after each.
fn tokens(sql: &str) -> Vec<(Tok, usize)> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let quoted = |i: usize, close: u8| -> (String, usize) {
        let mut s = Vec::new();
        let mut j = i + 1;
        while j < b.len() {
            if b[j] == close {
                if close != b']' && j + 1 < b.len() && b[j + 1] == close {
                    s.push(close);
                    j += 2;
                    continue;
                }
                return (String::from_utf8_lossy(&s).to_lowercase(), j + 1);
            }
            s.push(b[j]);
            j += 1;
        }
        (String::from_utf8_lossy(&s).to_lowercase(), b.len())
    };
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'-' && b.get(i + 1) == Some(&b'-') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i = sql[i + 2..]
                .find("*/")
                .map(|j| i + 2 + j + 2)
                .unwrap_or(b.len());
        } else if c == b'\'' {
            let (_, j) = quoted(i, b'\'');
            out.push((Tok::Other, j));
            i = j;
        } else if c == b'"' || c == b'`' || c == b'[' {
            let (s, j) = quoted(i, if c == b'[' { b']' } else { c });
            out.push((Tok::Name(s), j));
            i = j;
        } else if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
            let mut j = i;
            while j < b.len()
                && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'$' || b[j] >= 0x80)
            {
                j += 1;
            }
            out.push((Tok::Name(sql[i..j].to_lowercase()), j));
            i = j;
        } else {
            out.push((
                match c {
                    b'(' => Tok::Open,
                    b';' => Tok::Semi,
                    _ => Tok::Other,
                },
                i + 1,
            ));
            i += 1;
        }
    }
    out
}

/// Whether sql ends a statement (a semicolon inside CREATE TRIGGER ...
/// BEGIN ... END does not).
fn complete(sql: &str) -> bool {
    match CString::new(sql) {
        Ok(c) => unsafe { rusqlite::ffi::sqlite3_complete(c.as_ptr()) != 0 },
        Err(_) => true,
    }
}

/// Refuses statements of sql that would put one of the functions into the
/// schema: CREATE (TABLE, VIEW, TRIGGER, INDEX) and ALTER statements that
/// call one.
pub(crate) fn check_schema_sql(sql: &str, fns: &[ScriptFn]) -> Result<()> {
    if fns.is_empty() {
        return Ok(());
    }
    let toks = tokens(sql);
    let mut start = 0; // byte offset of the statement
    let mut first = 0; // its first token
    for i in 0..=toks.len() {
        let end = match toks.get(i) {
            Some((Tok::Semi, e)) if complete(&sql[start..*e]) => *e,
            Some(_) => continue,
            None => sql.len(),
        };
        let stmt = &toks[first..i];
        if matches!(stmt.first(), Some((Tok::Name(w), _)) if w == "create" || w == "alter") {
            for w in stmt.windows(2) {
                if let (Tok::Name(n), Tok::Open) = (&w[0].0, &w[1].0)
                    && let Some(f) = fns.iter().find(|f| f.name.to_lowercase() == *n)
                {
                    return Err(err!("sqlite.functionInSchema", f.name));
                }
            }
        }
        start = end;
        first = i + 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str) -> ScriptFn {
        ScriptFn {
            name: name.into(),
            id: 1,
            deterministic: false,
        }
    }

    #[test]
    fn schema_statements_are_checked() {
        let _l = crate::testutil::lock_lang("en");
        let fns = [f("calc")];
        let bad = [
            "CREATE VIEW v AS SELECT calc(a) FROM t",
            "create temp view v as select CALC (a) from t",
            "CREATE TABLE x(a, b AS (\"calc\"(a)))",
            "CREATE TABLE x(a DEFAULT (calc(1)))",
            "CREATE TRIGGER tr AFTER INSERT ON t BEGIN UPDATE t SET a = 1; INSERT INTO log VALUES(calc(new.a)); END",
            "SELECT calc(1); CREATE INDEX i ON t([calc](a))",
            "ALTER TABLE t ADD COLUMN z AS (calc(a))",
        ];
        for s in bad {
            let e = check_schema_sql(s, &fns).expect_err(s);
            assert!(e.to_string().contains("calc"), "{s}: {e}");
        }
        let good = [
            "SELECT calc(a) FROM t",
            "UPDATE t SET a = calc(a)",
            "CREATE TABLE x(a, calc INTEGER); SELECT calc(a) FROM x",
            "CREATE VIEW v AS SELECT 'calc(a)' AS s -- calc(\n FROM t",
            "CREATE TABLE x(calculated(a))",
            "CREATE TRIGGER tr AFTER INSERT ON t BEGIN UPDATE t SET a = 1; END; SELECT calc(1)",
        ];
        for s in good {
            assert!(check_schema_sql(s, &fns).is_ok(), "{s}");
        }
        assert!(check_schema_sql("CREATE VIEW v AS SELECT calc(1)", &[]).is_ok());
    }

    #[test]
    fn names() {
        for n in ["calc", "税込", "_x1"] {
            assert!(valid_name(n), "{n}");
        }
        for n in ["", "1x", "a b", "a(", "a-b"] {
            assert!(!valid_name(n), "{n}");
        }
    }
}
