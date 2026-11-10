//! Runs the script being developed in a process of its own (this exe), so
//! that a crash, exit() or an endless loop cannot take the development
//! window with it. Its output and its end come back as events.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::process::{Io, Running, Spec, Stream, Window};
use crate::Result;

/// What a run reports to the session.
#[derive(Debug)]
pub enum RunEvent {
    Output { id: u64, stderr: bool, text: String },
    Exited { id: u64, code: Option<i32> },
}

/// A running script.
pub struct Run {
    pub id: u64,
    proc: Arc<Running>,
}

/// The environment variable that tells a run it was started by the
/// development mode (fs.remove asks before deleting much).
pub const DEV_ENV: &str = "XLJS_DEV";
/// The environment variable with the file for the form preview.
pub const PREVIEW_ENV: &str = "XLJS_DEV_PREVIEW";

impl Run {
    /// Starts exe with args in dir; events go to tx (wrapped by wrap). The
    /// process runs in a job (crate::process): kill() also ends what the
    /// script started, and a program left holding its output does not
    /// keep the end from being reported.
    pub fn start<E: Send + 'static>(
        exe: &Path,
        args: &[String],
        dir: &Path,
        env: &[(String, String)],
        id: u64,
        tx: Sender<E>,
        wrap: fn(RunEvent) -> E,
    ) -> Result<Run> {
        let spec = Spec {
            command: exe.to_string_lossy().into_owned(),
            args: args.to_vec(),
            cwd: Some(dir.to_string_lossy().into_owned()),
            env: env.iter().map(|(k, v)| (k.clone(), Some(v.clone()))).collect(),
            window: Window::Hidden,
            stdin: Io::Ignore,
            discard_output: true,
            ..Default::default()
        };
        let tx = Arc::new(Mutex::new(tx));
        let text = Arc::new(Mutex::new(Decoders::default()));
        let (t1, x1) = (tx.clone(), text.clone());
        let proc = Running::start_chunks(
            &spec,
            Arc::new(move |which, b: &[u8]| {
                let stderr = which == Stream::Stderr;
                let s = x1.lock().unwrap().push(stderr, b);
                if !s.is_empty() {
                    let _ = t1.lock().unwrap().send(wrap(RunEvent::Output { id, stderr, text: s }));
                }
            }),
        )?;
        proc.when_done(move |d| {
            let tx = tx.lock().unwrap();
            for stderr in [false, true] {
                let s = text.lock().unwrap().finish(stderr);
                if !s.is_empty() {
                    let _ = tx.send(wrap(RunEvent::Output { id, stderr, text: s }));
                }
            }
            let _ = tx.send(wrap(RunEvent::Exited { id, code: d.code }));
        });
        Ok(Run { id, proc })
    }

    /// Ends the process and what it started (the Exited event follows).
    pub fn kill(&self) {
        self.proc.kill();
    }

    pub fn running(&self) -> bool {
        self.proc.running()
    }
}

/// Turns the output of both streams into text as it arrives. UTF-8 is
/// given as soon as its characters are whole; other text (CP932 from the
/// programs a script runs) by whole lines, so that no character is cut.
#[derive(Default)]
struct Decoders {
    pending: [Vec<u8>; 2],
}

/// How much text that is not UTF-8 waits for the end of its line.
const MAX_LINE: usize = 4096;

impl Decoders {
    fn push(&mut self, stderr: bool, b: &[u8]) -> String {
        let p = &mut self.pending[stderr as usize];
        p.extend_from_slice(b);
        let cut = match std::str::from_utf8(p) {
            Ok(_) => p.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => match p.iter().rposition(|&c| c == b'\n') {
                Some(i) => i + 1,
                None if p.len() > MAX_LINE => p.len(),
                None => 0,
            },
        };
        let text = super::project::decode(&p[..cut]);
        p.drain(..cut);
        text
    }

    fn finish(&mut self, stderr: bool) -> String {
        let p = std::mem::take(&mut self.pending[stderr as usize]);
        super::project::decode(&p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_keeps_characters_whole() {
        let mut d = Decoders::default();
        let utf8 = "あい".as_bytes();
        assert_eq!(d.push(false, &utf8[..4]), "あ");
        assert_eq!(d.push(false, &utf8[4..]), "い");
        // CP932 cut inside a character: given by line.
        let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("表示\n次");
        assert_eq!(d.push(true, &sjis[..3]), "");
        assert_eq!(d.push(true, &sjis[3..]), "表示\n");
        assert_eq!(d.finish(true), "次");
        assert_eq!(d.finish(false), "");
    }
}
