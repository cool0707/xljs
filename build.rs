//! Generates the embedded files (the built-in web pages, Day.js, the
//! message files, prelude.js and other large texts; see src/embed.rs) and,
//! for Windows targets, the exe resources (icon, manifest).
//!
//! Each file is deflate-compressed into OUT_DIR/embed/ and included from
//! there; files that do not get smaller (fonts, icons) are included as they
//! are.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

struct Gen {
    out: String,
    dir: PathBuf,
    count: usize,
    raw: usize,
    stored: usize,
}

impl Gen {
    /// A `File::new(...)` expression for the file at path, named name.
    fn file(&mut self, name: &str, path: &Path) -> String {
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        z.write_all(&data).unwrap();
        let packed = z.finish().unwrap();
        // Keep the compressed copy only when it saves at least a tenth.
        let deflated = packed.len() < data.len() - data.len() / 10;
        let src = if deflated {
            let p = self.dir.join(format!("{}.deflate", self.count));
            std::fs::write(&p, &packed).unwrap();
            self.count += 1;
            p
        } else {
            path.to_path_buf()
        };
        self.raw += data.len();
        self.stored += if deflated { packed.len() } else { data.len() };
        format!("File::new({name:?}, include_bytes!({:?}), {}, {deflated})", src.to_string_lossy(), data.len())
    }

    /// `pub static NAME: [File; n]` with the files under dir (paths
    /// relative to it), those for which keep returns true.
    fn dir(&mut self, name: &str, dir: &str, keep: impl Fn(&str) -> bool) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir);
        println!("cargo:rerun-if-changed={dir}");
        let mut items = Vec::new();
        for f in files(&root) {
            let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            if keep(&rel) {
                items.push(self.file(&rel, &f));
            }
        }
        writeln!(self.out, "pub static {name}: [File; {}] = [", items.len()).unwrap();
        for i in items {
            writeln!(self.out, "    {i},").unwrap();
        }
        writeln!(self.out, "];").unwrap();
    }

    /// `pub static NAME: File` with one file.
    fn single(&mut self, name: &str, path: &str) {
        println!("cargo:rerun-if-changed={path}");
        let f = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
        let base = f.file_name().unwrap().to_string_lossy().into_owned();
        let item = self.file(&base, &f);
        writeln!(self.out, "pub static {name}: File = {item};").unwrap();
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=XLJS_VERSION");
    let dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embed");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut g = Gen { out: String::new(), dir, count: 0, raw: 0, stored: 0 };
    g.dir("WEB_FILES", "src/ui/web", |_| true);
    g.dir("DAYJS_FILES", "src/api/dayjs", |_| true);
    g.dir("LOCALE_FILES", "src/i18n/locales", |n| n.ends_with(".json"));
    g.single("PRELUDE", "src/api/prelude.js");
    g.single("PRESET_SHAPES", "src/api/preset_shapes.xml");
    g.single("PRESET_TEXT_WARPS", "src/api/preset_text_warps.xml");
    g.single("DTS", "types/xljs.d.ts");
    // The documents for AI chats (the prompt, the reference and the skills).
    g.dir("AI_FILES", "docs/ai", |n| n.ends_with(".md") && n != "README.md");
    writeln!(g.out, "// {} bytes stored for {} bytes of files", g.stored, g.raw).unwrap();
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded.rs");
    std::fs::write(out, g.out).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=src/cli/xljs.rc");
        println!("cargo:rerun-if-changed=src/cli/xljs.manifest");
        println!("cargo:rerun-if-changed=assets/xljs.ico");
        println!("cargo:rerun-if-changed=assets/xljs-file.ico");
        embed_resource::compile("src/cli/xljs.rc", embed_resource::NONE).manifest_required().unwrap();
    }
}
