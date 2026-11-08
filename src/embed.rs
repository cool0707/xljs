//! Files built into the executable: the built-in web pages (src/ui/web),
//! Day.js, the message files, prelude.js, the preset shape definitions, the
//! type definitions and the AI prompt.
//!
//! build.rs stores them deflate-compressed (except files that do not get
//! smaller, such as fonts) to keep the exe small; a file is decompressed on
//! first use and kept for the rest of the process.

use std::io::Read;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// One embedded file.
pub struct File {
    /// The path relative to the embedded folder (e.g. "vendor/x.js"), or the
    /// file name for a single file.
    pub name: &'static str,
    data: &'static [u8],
    size: usize,
    deflated: bool,
    cache: OnceLock<Box<[u8]>>,
}

impl File {
    const fn new(name: &'static str, data: &'static [u8], size: usize, deflated: bool) -> File {
        File { name, data, size, deflated, cache: OnceLock::new() }
    }

    /// The file's contents.
    pub fn bytes(&'static self) -> &'static [u8] {
        if !self.deflated {
            return self.data;
        }
        self.cache.get_or_init(|| {
            let mut out = Vec::with_capacity(self.size);
            flate2::read::DeflateDecoder::new(self.data)
                .read_to_end(&mut out)
                .unwrap_or_else(|e| panic!("embedded file {}: {e}", self.name));
            out.into_boxed_slice()
        })
    }

    /// The file's contents as text (all embedded texts are UTF-8).
    pub fn text(&'static self) -> &'static str {
        std::str::from_utf8(self.bytes()).unwrap_or_else(|e| panic!("embedded file {}: {e}", self.name))
    }
}

/// The file named name in files.
pub fn find(files: &'static [File], name: &str) -> Option<&'static File> {
    files.iter().find(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every embedded file decompresses to its original contents.
    #[test]
    fn embedded_files_match_sources() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let dirs: [(&'static [File], &str); 3] =
            [(&WEB_FILES, "src/ui/web"), (&DAYJS_FILES, "src/api/dayjs"), (&LOCALE_FILES, "src/i18n/locales")];
        let mut compressed = 0;
        for (files, dir) in dirs {
            assert!(!files.is_empty(), "{dir}");
            for f in files {
                let want = std::fs::read(root.join(dir).join(f.name)).unwrap();
                assert_eq!(f.bytes(), &want[..], "{dir}/{}", f.name);
                compressed += f.deflated as usize;
            }
        }
        for (f, path) in [
            (&PRELUDE, "src/api/prelude.js"),
            (&PRESET_SHAPES, "src/api/preset_shapes.xml"),
            (&PRESET_TEXT_WARPS, "src/api/preset_text_warps.xml"),
            (&DTS, "types/xljs.d.ts"),
            (&AI_PROMPT, "docs/ai/prompt.md"),
        ] {
            assert_eq!(f.text(), std::fs::read_to_string(root.join(path)).unwrap(), "{path}");
            assert!(f.deflated, "{path}");
        }
        assert!(compressed > 0);
        assert!(find(&WEB_FILES, "form.html").is_some());
        assert!(find(&WEB_FILES, "nothing.html").is_none());
    }
}
