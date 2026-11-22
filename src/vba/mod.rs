//! Reads the VBA source code of Excel macro workbooks: the VBA project
//! (vbaProject.bin) of .xlsm/.xlsb/.xlam/.xltm files, and the
//! _VBA_PROJECT_CUR storage of the old binary .xls/.xla files.
//!
//! The project is an OLE compound file ([MS-CFB]) laid out as [MS-OVBA]
//! describes; umya-spreadsheet's `vba` module (shared with RAS) reads it:
//! the "dir" stream that lists the modules, and every module's source,
//! compressed after the compiled code and stored in the project's code page
//! (932 for Japanese Excel). What is here is the container around it (the
//! zip of a workbook, the compound file of an .xls) and the names of the
//! sheets the document modules belong to.

mod sheets;
mod text;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Component;

use umya_spreadsheet::vba::{ModuleKind, VbaError, VbaProject};

use crate::{Error, Result, err};

pub use text::text;

/// Module types.
pub const STANDARD: &str = "standard"; // standard module (Module1)
pub const CLASS: &str = "class"; // class module
pub const DOCUMENT: &str = "document"; // ThisWorkbook and the sheets' modules
pub const FORM: &str = "form"; // user form (its code only; the layout is not read)

/// One VBA module.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Module {
    pub name: String,
    pub kind: String,
    /// The source with LF line endings, including the Attribute lines at
    /// the top.
    pub code: String,
    /// The name of the sheet a document module belongs to, when it is known
    /// ("" for ThisWorkbook and for .xls/.xlsb files).
    pub sheet: String,
}

/// The VBA project of a workbook.
#[derive(Debug, Clone, Default)]
pub struct Project {
    pub name: String,
    pub code_page: u32,
    pub modules: Vec<Module>,
}

const CFB_SIGNATURE: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";

/// Reads the VBA project of the workbook at path. It returns None (and no
/// error) when the workbook has no macros.
pub fn read_file(path: &str) -> Result<Option<Project>> {
    let data = std::fs::read(path).map_err(|e| Error::path("open", path, e))?;
    read(&data).map_err(|e| e.context(path))
}

/// Reads the VBA project of a workbook (or of a vbaProject.bin) held in
/// memory. It returns None (and no error) when there are no macros.
pub fn read(data: &[u8]) -> Result<Option<Project>> {
    if data.starts_with(ZIP_SIGNATURE) {
        read_zip(data)
    } else if data.starts_with(CFB_SIGNATURE) {
        read_cfb(data, &HashMap::new())
    } else {
        Err(err!("vba.unsupported"))
    }
}

fn read_zip(data: &[u8]) -> Result<Option<Project>> {
    let mut zr = zip::ZipArchive::new(Cursor::new(data)).map_err(|_| err!("vba.unsupported"))?;
    let bin = zr.file_names().find(|n| path_base(n).eq_ignore_ascii_case("vbaProject.bin")).map(str::to_string);
    let Some(bin) = bin else {
        return Ok(None);
    };
    let f = zr.by_name(&bin).map_err(|e| Error::msg(e.to_string()))?;
    let b = crate::zippart::read_file(f).ok_or_else(|| corrupt("vbaProject.bin"))?;
    let sheets = sheets::sheet_code_names(&mut zr);
    read_cfb(&b, &sheets)
}

fn path_base(name: &str) -> &str {
    match name.rfind(['/', '\\']) {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn corrupt(s: impl std::fmt::Display) -> Error {
    err!("vba.corrupt", s)
}

fn read_cfb(data: &[u8], sheets: &HashMap<String, String>) -> Result<Option<Project>> {
    // The project is the storage holding VBA/dir: the root of a
    // vbaProject.bin, _VBA_PROJECT_CUR in an .xls.
    let doc = cfb::CompoundFile::open(Cursor::new(data)).map_err(corrupt)?;
    let mut root = None;
    for e in doc.walk() {
        if !e.is_stream() {
            continue;
        }
        if e.name() == "EncryptedPackage" {
            return Err(err!("vba.encrypted"));
        }
        let storages: Vec<&str> = e
            .path()
            .parent()
            .map(|p| p.components().filter_map(|c| if let Component::Normal(n) = c { n.to_str() } else { None }).collect())
            .unwrap_or_default();
        if root.is_none() && e.name().eq_ignore_ascii_case("dir") && storages.last().is_some_and(|s| s.eq_ignore_ascii_case("VBA")) {
            root = Some(storages[..storages.len() - 1].join("/"));
        }
    }
    let Some(root) = root else {
        return Ok(None);
    };
    let project = VbaProject::read_storage(data, &root).map_err(|e| match e {
        VbaError::MissingModuleStream { module, .. } => err!("vba.moduleMissing", module),
        VbaError::Format(m) => corrupt(m),
        VbaError::Cfb(e) => corrupt(e),
        e => corrupt(e),
    })?;
    let mut p = Project { name: project.name().to_string(), code_page: project.codepage().into(), modules: Vec::new() };
    for m in project.modules() {
        let kind = match m.kind() {
            ModuleKind::Standard => STANDARD,
            ModuleKind::Class => CLASS,
            ModuleKind::Document => DOCUMENT,
            ModuleKind::Form => FORM,
        };
        let mut module = Module { name: m.name().to_string(), kind: kind.to_string(), code: normalize_newlines(m.source()), ..Default::default() };
        if module.kind == DOCUMENT {
            module.sheet = lookup_fold(sheets, &module.name);
        }
        p.modules.push(module);
    }
    Ok(Some(p))
}

fn lookup_fold(m: &HashMap<String, String>, key: &str) -> String {
    if let Some(v) = m.get(key) {
        return v.clone();
    }
    let lower = key.to_lowercase();
    m.iter().find(|(k, _)| k.to_lowercase() == lower).map(|(_, v)| v.clone()).unwrap_or_default()
}

fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests;
