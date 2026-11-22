//! Helpers for the tests: the process-wide state (language, working
//! folder, environment) is shared, so tests that touch it take the lock.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicU64, Ordering};

static LOCK: Mutex<()> = Mutex::new(());

/// Holds the global test lock with the language set; the language goes
/// back to Japanese (what the script tests expect) when it is dropped.
pub struct LangGuard {
    _g: MutexGuard<'static, ()>,
}

impl Drop for LangGuard {
    fn drop(&mut self) {
        crate::i18n::set_language("ja");
    }
}

pub fn lock_lang(lang: &str) -> LangGuard {
    let g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::i18n::set_language(lang);
    LangGuard { _g: g }
}

/// A new empty temporary folder.
pub fn tempdir() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!("xljs-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// A path in the repository.
pub fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// A buffer that several threads can write to (logs, console output).
#[derive(Clone, Default)]
pub struct SharedBuf(pub std::sync::Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
    /// A mock log sink appending lines to the buffer.
    pub fn sink(&self) -> crate::ui::mock::LogSink {
        let b = self.clone();
        std::sync::Arc::new(move |s: &str| {
            let mut v = b.0.lock().unwrap();
            v.extend_from_slice(s.as_bytes());
            v.push(b'\n');
        })
    }
}

impl std::io::Write for SharedBuf {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// An xlsx file made by umya-spreadsheet, with `edit` applied to the text of
/// its parts (name, text).
pub fn xlsx_edited(edit: impl Fn(&str, String) -> String) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut file = std::io::Cursor::new(Vec::new());
    umya_spreadsheet::writer::xlsx::write_writer(&umya_spreadsheet::new_file(), &mut file).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(file.into_inner())).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let data = match String::from_utf8(data) {
            Ok(text) => edit(&name, text).into_bytes(),
            Err(e) => e.into_bytes(),
        };
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// An xlsx file whose first sheet holds `body` (what goes in `<worksheet>`).
pub fn xlsx_with_sheet(body: &str) -> Vec<u8> {
    xlsx_edited(|name, text| {
        if name != "xl/worksheets/sheet1.xml" {
            return text;
        }
        let start = text.find("<worksheet").and_then(|i| text[i..].find('>').map(|j| i + j + 1)).unwrap();
        let end = text.rfind("</worksheet>").unwrap();
        format!("{}{}{}", &text[..start], body, &text[end..])
    })
}
