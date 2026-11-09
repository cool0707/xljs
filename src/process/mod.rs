//! Running programs (shell.run / shell.runSync / shell.spawn): the start of
//! the process, the threads that read its output and wait for it, and the
//! result. Windows starts the process with CreateProcessW (`win.rs`: the
//! window state, no console window, a job object so that kill() ends the
//! programs it started too); other systems with std::process (`unix.rs`).

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::{Error, Result, err};

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
use unix as sys;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as sys;

#[cfg(test)]
mod tests;

/// Where a standard stream of the child goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Io {
    /// Read (stdout, stderr) or written (stdin) by xljs.
    #[default]
    Pipe,
    /// The stream of xljs itself.
    Inherit,
    /// Nothing (NUL).
    Ignore,
}

/// The window of the program (Windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Window {
    /// Console programs without a window (or in the console of xljs when
    /// it has one), other programs as they open themselves.
    #[default]
    Default,
    Normal,
    Hidden,
    Minimized,
    Maximized,
}

/// What to run (built by the prelude).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Spec {
    pub command: String,
    pub args: Vec<String>,
    /// Runs command (a command line) with the shell: cmd.exe /c on
    /// Windows, sh -c elsewhere. args are added to the line, quoted.
    pub shell: bool,
    pub cwd: Option<String>,
    /// Variables to set (null removes one) on top of the environment.
    pub env: HashMap<String, Option<String>>,
    pub window: Window,
    pub stdin: Io,
    pub stdout: Io,
    pub stderr: Io,
    /// Milliseconds after which the program is ended (0 = none).
    pub timeout: f64,
    /// The encoding of the output for the line callbacks ("auto" detects
    /// UTF-8 or CP932 by line).
    pub encoding: String,
}

impl Spec {
    pub fn timeout(&self) -> Option<Duration> {
        (self.timeout.is_finite() && self.timeout > 0.0).then(|| Duration::from_secs_f64(self.timeout / 1000.0))
    }
    fn line_encoding(&self) -> &str {
        match self.encoding.as_str() {
            "" | "buffer" => "auto",
            e => e,
        }
    }
}

/// The streams of a started process (the parent's ends of the pipes).
pub(crate) struct Started {
    pub pid: u32,
    pub handle: sys::Handle,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}

/// Which output a line came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Receives the lines of the output as they arrive (on a reader thread).
pub type OnLines = Arc<dyn Fn(Stream, Vec<String>) + Send + Sync>;

/// The end of a process: its exit code (None when it was ended by a
/// signal) and everything it wrote.
#[derive(Clone, Debug, Default)]
pub struct Done {
    pub code: Option<i32>,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

type Waiter = Box<dyn FnOnce(&Done) + Send>;

#[derive(Default)]
struct State {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    readers: u32,
    exit: Option<(Option<i32>, bool)>,
    done: Option<Done>,
    waiters: Vec<Waiter>,
}

/// How long the output may still arrive after the process ended (a program
/// it started can hold the pipe open).
const GRACE: Duration = Duration::from_millis(500);

/// A running (or ended) process.
pub struct Running {
    pub pid: u32,
    handle: sys::Handle,
    st: Mutex<State>,
    cv: Condvar,
    stdin: Mutex<Option<File>>,
}

impl Running {
    /// Starts the program. input (when given) is written to its standard
    /// input, which is then closed; otherwise with spec.stdin = Pipe the
    /// caller writes with write() and closes with end().
    pub fn start(spec: &Spec, input: Option<Vec<u8>>, on_lines: Option<OnLines>) -> Result<Arc<Running>> {
        if spec.command.trim().is_empty() {
            return Err(err!("process.noCommand"));
        }
        let mut spec = spec.clone();
        if input.is_some() {
            spec.stdin = Io::Pipe;
        }
        if let Some(d) = &spec.cwd {
            match std::fs::metadata(d) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(Error::path("chdir", d.clone(), std::io::Error::from(std::io::ErrorKind::NotADirectory))),
                Err(e) => return Err(Error::path("chdir", d.clone(), e)),
            }
        }
        let s = sys::spawn(&spec)?;
        let r = Arc::new(Running {
            pid: s.pid,
            handle: s.handle,
            st: Mutex::new(State::default()),
            cv: Condvar::new(),
            stdin: Mutex::new(None),
        });
        match (s.stdin, input) {
            (Some(mut f), Some(data)) => {
                std::thread::spawn(move || {
                    let _ = f.write_all(&data);
                });
            }
            (Some(f), None) => *r.stdin.lock().unwrap() = Some(f),
            _ => {}
        }
        let enc = spec.line_encoding().to_string();
        for (f, which) in [(s.stdout, Stream::Stdout), (s.stderr, Stream::Stderr)] {
            let Some(f) = f else { continue };
            r.st.lock().unwrap().readers += 1;
            let (r1, cb, enc) = (r.clone(), on_lines.clone(), enc.clone());
            std::thread::spawn(move || r1.read(f, which, cb, &enc));
        }
        let (r1, timeout) = (r.clone(), spec.timeout());
        std::thread::spawn(move || r1.watch(timeout));
        Ok(r)
    }

    fn read(&self, mut f: File, which: Stream, on_lines: Option<OnLines>, enc: &str) {
        let mut buf = vec![0u8; 16 * 1024];
        let mut lines = LineSplitter::default();
        loop {
            let n = match f.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            {
                let mut st = self.st.lock().unwrap();
                match which {
                    Stream::Stdout => st.stdout.extend_from_slice(&buf[..n]),
                    Stream::Stderr => st.stderr.extend_from_slice(&buf[..n]),
                }
            }
            if let Some(cb) = &on_lines {
                let ls = lines.push(&buf[..n]);
                if !ls.is_empty() {
                    cb(which, ls.iter().map(|l| decode_line(l, enc)).collect());
                }
            }
        }
        if let Some(cb) = &on_lines
            && let Some(l) = lines.finish()
        {
            cb(which, vec![decode_line(&l, enc)]);
        }
        drop(on_lines);
        let mut st = self.st.lock().unwrap();
        st.readers -= 1;
        self.cv.notify_all();
    }

    /// Waits for the process (ending it after timeout), then for the
    /// output, and calls the waiters.
    fn watch(&self, timeout: Option<Duration>) {
        let (code, timed_out) = match self.handle.wait(timeout) {
            Some(code) => (code, false),
            None => {
                self.handle.kill();
                (self.handle.wait(None).flatten(), true)
            }
        };
        let deadline = Instant::now() + GRACE;
        let mut st = self.st.lock().unwrap();
        st.exit = Some((code, timed_out));
        while st.readers > 0 {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            st = self.cv.wait_timeout(st, deadline - now).unwrap().0;
        }
        let done = Done { code, timed_out, stdout: st.stdout.clone(), stderr: st.stderr.clone() };
        st.done = Some(done.clone());
        let waiters = std::mem::take(&mut st.waiters);
        self.cv.notify_all();
        drop(st);
        self.stdin.lock().unwrap().take();
        for w in waiters {
            w(&done);
        }
    }

    /// Calls f with the result once the process has ended (right away when
    /// it has; otherwise on the thread that waits for it).
    pub fn when_done(&self, f: impl FnOnce(&Done) + Send + 'static) {
        let mut st = self.st.lock().unwrap();
        match &st.done {
            Some(d) => {
                let d = d.clone();
                drop(st);
                f(&d);
            }
            None => st.waiters.push(Box::new(f)),
        }
    }

    /// The result, once the process has ended and its output has been read.
    pub fn done(&self) -> Option<Done> {
        self.st.lock().unwrap().done.clone()
    }

    /// Waits up to d for the end; the result when it has ended.
    pub fn wait_timeout(&self, d: Duration) -> Option<Done> {
        let st = self.st.lock().unwrap();
        let st = self.cv.wait_timeout_while(st, d, |s| s.done.is_none()).unwrap().0;
        st.done.clone()
    }

    pub fn wait(&self) -> Done {
        let st = self.st.lock().unwrap();
        let st = self.cv.wait_while(st, |s| s.done.is_none()).unwrap();
        st.done.clone().unwrap_or_default()
    }

    /// Whether the process is still running (its output can still be read
    /// after it ended).
    pub fn running(&self) -> bool {
        self.st.lock().unwrap().exit.is_none()
    }

    /// Ends the process and the programs it started.
    pub fn kill(&self) {
        if self.running() {
            self.handle.kill();
        }
    }

    /// Writes to the standard input (spec.stdin = Pipe).
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut g = self.stdin.lock().unwrap();
        let Some(f) = g.as_mut() else {
            return Err(err!("process.stdinClosed"));
        };
        f.write_all(data).and_then(|_| f.flush()).map_err(|e| {
            g.take();
            Error::from(e)
        })
    }

    /// Closes the standard input (the program sees its end).
    pub fn end(&self) {
        self.stdin.lock().unwrap().take();
    }
}

/// Splits output into lines at "\n", "\r\n" and a lone "\r" (progress
/// shown in place), keeping the bytes of a line that has not ended.
#[derive(Default)]
struct LineSplitter {
    part: Vec<u8>,
    cr: bool,
}

impl LineSplitter {
    fn push(&mut self, b: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for &c in b {
            if self.cr {
                self.cr = false;
                if c == b'\n' {
                    continue;
                }
            }
            match c {
                b'\n' => out.push(std::mem::take(&mut self.part)),
                b'\r' => {
                    out.push(std::mem::take(&mut self.part));
                    self.cr = true;
                }
                _ => self.part.push(c),
            }
        }
        out
    }

    fn finish(&mut self) -> Option<Vec<u8>> {
        (!self.part.is_empty()).then(|| std::mem::take(&mut self.part))
    }
}

/// Decodes output: "auto" takes UTF-8 when it is valid UTF-8 (or has a
/// BOM), else CP932 (what Japanese Windows console programs write).
pub fn decode_output(b: &[u8], enc: &str) -> String {
    let enc = if enc.is_empty() { "auto" } else { enc };
    crate::api::decode(b, enc).unwrap_or_else(|_| String::from_utf8_lossy(b).into_owned())
}

fn decode_line(b: &[u8], enc: &str) -> String {
    decode_output(b, enc)
}
