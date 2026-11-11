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
    paths: Vec<String>,
}

fn parse(args: &[String]) -> Result<Args> {
    let mut a = Args::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
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

pub(super) fn cmd_test(con: &ConsoleState, args: &[String]) -> i32 {
    let a = match parse(args) {
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
        assert!(parse(&args(&["--timeout", "0", "a.js"])).is_err());
        assert!(parse(&args(&["--only"])).is_err());
        assert!(parse(&args(&["--nope", "a.js"])).is_err());
    }
}
