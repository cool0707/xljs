//! `xljs test`: replays the test cases of a script (see testing) and tells
//! which runs differ from what was approved.

use std::time::Duration;

use super::console::ConsoleState;
use crate::testing::{self, Outcome, RunOptions, Status};
use crate::{Error, Result, err, t};

#[derive(Default)]
struct Args {
    approve: bool,
    json: bool,
    only: String,
    limit: Option<Duration>,
    new: Option<String>,
    /// --suggest: the case to add variants to.
    suggest: Option<String>,
    /// --record: the name of the case; the paths after the script are its arguments.
    record: Option<String>,
    paths: Vec<String>,
}

fn parse(args: &[String]) -> Result<Args> {
    let mut a = Args::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if a.record.is_some() && !a.paths.is_empty() {
            // The arguments of the script.
            a.paths.extend_from_slice(&args[i..]);
            break;
        }
        let (name, inline) = match arg.strip_prefix("--").or_else(|| arg.strip_prefix('-')).filter(|_| arg.len() > 1) {
            Some(f) => match f.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (f, None),
            },
            None => {
                a.paths.push(arg.clone());
                i += 1;
                continue;
            }
        };
        let mut value = || -> Result<String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            i += 1;
            args.get(i).cloned().ok_or_else(|| Error::msg(format!("flag needs an argument: --{name}")))
        };
        match name {
            "approve" => a.approve = true,
            "json" => a.json = true,
            "only" => a.only = value()?,
            "new" => a.new = Some(value()?),
            "record" => a.record = Some(value()?),
            "suggest" => a.suggest = Some(value()?),
            "timeout" => {
                let v = value()?;
                match v.parse::<f64>() {
                    Ok(s) if s > 0.0 && s.is_finite() => a.limit = Some(Duration::from_secs_f64(s)),
                    _ => return Err(err!("test.badTimeout", v)),
                }
            }
            _ => return Err(Error::msg(format!("flag provided but not defined: {arg}"))),
        }
        i += 1;
    }
    Ok(a)
}

fn label(o: &Outcome) -> String {
    if o.approved {
        return t!("test.approved");
    }
    match o.status {
        Status::Pass => t!("test.pass"),
        Status::Fail => t!("test.fail"),
        Status::New => t!("test.new"),
        Status::Problem => t!("test.problem"),
    }
}

fn print(o: &Outcome) {
    println!("{}  {}", label(o), o.name());
    if o.status == Status::Fail && !o.approved {
        println!("  {}", t!("test.diffLegend"));
    }
    if !o.approved || o.status == Status::Problem {
        for l in &o.lines {
            println!("  {l}");
        }
    }
}

/// Runs the script with the UI and records the answers as a case.
fn record(con: &ConsoleState, name: &str, a: &Args, ui_mode: &str, answers: &str) -> i32 {
    let script = &a.paths[0];
    let made = (|| -> Result<(std::sync::Arc<dyn crate::ui::Ui>, crate::engine::Options)> {
        let u = super::choose_ui(ui_mode, answers)?;
        let m = super::read_meta(script)?;
        super::apply_lang(&m, false);
        let mut o = crate::engine::Options { meta: Some(m), version: super::version(), ..Default::default() };
        super::set_stop(&mut o, &super::Stop::from_env(), &*u)?;
        Ok((u, o))
    })();
    let (u, o) = match made {
        Ok(x) => x,
        Err(e) => return con.fail(None, &e),
    };
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let r = match testing::record(u.clone(), script, name, &a.paths[1..], &now, o) {
        Ok(r) => r,
        Err(e) => return con.fail(Some(&*u), &e),
    };
    let code = match &r.result {
        Ok(c) => *c,
        Err(e) => con.fail(Some(&*u), e),
    };
    for f in &r.failed {
        eprintln!("{}", t!("test.notCopied", f));
    }
    println!("{}", t!("test.recorded", r.path.display()));
    code
}

pub(super) fn cmd_test(con: &ConsoleState, args: &[String], ui_mode: &str, answers: &str) -> i32 {
    let a = match parse(args) {
        Ok(a) if a.record.is_some() && !a.paths.is_empty() => {
            let name = a.record.clone().unwrap_or_default();
            return record(con, &name, &a, ui_mode, answers);
        }
        Ok(a) if a.paths.len() == 1 => a,
        Ok(_) => {
            eprintln!("{}", t!("cli.usageTest"));
            return 2;
        }
        Err(e) => {
            eprintln!("{}", t!("cli.usageTest"));
            return con.fail(None, &e);
        }
    };
    if let Some(name) = &a.new {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        return match testing::new_case(&a.paths[0], name, &today) {
            Ok(p) => {
                println!("{}", t!("cli.created", p.display()));
                0
            }
            Err(e) => con.fail(None, &e),
        };
    }
    let o = RunOptions { approve: a.approve, only: a.only.clone(), limit: a.limit, version: super::version() };
    if let Some(name) = &a.suggest {
        return match testing::suggest(&a.paths[0], name, &o) {
            Ok(added) => {
                for n in &added {
                    println!("{}/{n}", name);
                }
                println!("{}", t!("test.suggested", added.len()));
                0
            }
            Err(e) => con.fail(None, &e),
        };
    }
    let json = a.json;
    let outcomes = match testing::run_all(&a.paths[0], &o, |r| {
        if !json {
            print(r);
        }
    }) {
        Ok(v) => v,
        Err(e) => return con.fail(None, &e),
    };
    let count = |f: &dyn Fn(&Outcome) -> bool| outcomes.iter().filter(|r| f(r)).count();
    let pass = count(&|r| r.approved || r.status == Status::Pass);
    let fail = count(&|r| !r.approved && r.status == Status::Fail);
    let new = count(&|r| !r.approved && r.status == Status::New);
    let problem = count(&|r| r.status == Status::Problem);
    if json {
        let doc = serde_json::json!({ "pass": pass, "fail": fail, "new": new, "problem": problem, "outcomes": outcomes });
        println!("{doc}");
    } else {
        println!("{}", t!("test.summary", pass, fail, new, problem));
        if fail + new > 0 {
            println!("{}", t!("test.approveHint"));
        }
    }
    if outcomes.iter().all(Outcome::ok) { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_args() {
        let a = parse(&args(&["--approve", "--only", "月次/商品別", "--timeout=2.5", "a.js", "--json"])).unwrap();
        assert!(a.approve && a.json);
        assert_eq!(a.only, "月次/商品別");
        assert_eq!(a.limit, Some(Duration::from_millis(2500)));
        assert_eq!(a.paths, ["a.js"]);
        let a = parse(&args(&["--new=月次", "a.js"])).unwrap();
        assert_eq!(a.new.as_deref(), Some("月次"));
        let a = parse(&args(&["--record", "月次", "a.js", "--x", "in.xlsx"])).unwrap();
        assert_eq!((a.record.as_deref(), a.paths.len()), (Some("月次"), 3));
        assert_eq!(a.paths[1], "--x");
        assert!(parse(&args(&["--timeout", "0", "a.js"])).is_err());
        assert!(parse(&args(&["--only"])).is_err());
        assert!(parse(&args(&["--nope", "a.js"])).is_err());
    }
}
