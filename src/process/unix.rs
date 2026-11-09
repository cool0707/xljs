//! Starting processes with std::process (systems other than Windows; the
//! window state does not apply).

use std::fs::File;
use std::os::fd::OwnedFd;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{Io, Spec, Started};
use crate::{Error, Result};

pub(crate) struct Handle(Mutex<Child>);

impl Handle {
    /// The exit code (None when ended by a signal), or None when timeout
    /// passed first.
    pub fn wait(&self, timeout: Option<Duration>) -> Option<Option<i32>> {
        let end = timeout.map(|d| Instant::now() + d);
        loop {
            if let Ok(Some(st)) = self.0.lock().unwrap().try_wait() {
                return Some(st.code());
            }
            if end.is_some_and(|e| Instant::now() >= e) {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn kill(&self) {
        let _ = self.0.lock().unwrap().kill();
    }
}

fn stdio(io: Io) -> Stdio {
    match io {
        Io::Pipe => Stdio::piped(),
        Io::Inherit => Stdio::inherit(),
        Io::Ignore => Stdio::null(),
    }
}

pub(crate) fn shell_quote(a: &str) -> Result<String> {
    if !a.is_empty()
        && a.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_./=:,+@%".contains(&c))
    {
        return Ok(a.to_string());
    }
    Ok(format!("'{}'", a.replace('\'', r"'\''")))
}

pub(crate) fn spawn(spec: &Spec) -> Result<Started> {
    let mut cmd = if spec.shell {
        let mut line = spec.command.clone();
        for a in &spec.args {
            line.push(' ');
            line.push_str(&shell_quote(a)?);
        }
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(line);
        c
    } else {
        let mut c = Command::new(&spec.command);
        c.args(&spec.args);
        c
    };
    if let Some(d) = &spec.cwd {
        cmd.current_dir(d);
    }
    for (k, v) in &spec.env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    cmd.stdin(stdio(spec.stdin))
        .stdout(stdio(spec.stdout))
        .stderr(stdio(spec.stderr));
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::path("exec", spec.command.clone(), e))?;
    let stdin = child.stdin.take().map(|s| File::from(OwnedFd::from(s)));
    let stdout = child.stdout.take().map(|s| File::from(OwnedFd::from(s)));
    let stderr = child.stderr.take().map(|s| File::from(OwnedFd::from(s)));
    Ok(Started {
        pid: child.id(),
        handle: Handle(Mutex::new(child)),
        stdin,
        stdout,
        stderr,
    })
}
