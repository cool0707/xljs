//! Reading the files (parts) of a zip archive: a workbook, a packed exe.
//! A part is read up to MAX_PART bytes, so that a small archive that
//! claims a huge part (a zip bomb) cannot take all the memory.

use std::io::{Read, Seek};

use zip::ZipArchive;

/// The largest part read.
pub(crate) const MAX_PART: u64 = 1 << 30;

/// The contents of a part already opened; None when it cannot be read or
/// is larger than MAX_PART.
pub(crate) fn read_file(f: impl Read) -> Option<Vec<u8>> {
    let mut b = Vec::new();
    f.take(MAX_PART + 1).read_to_end(&mut b).ok()?;
    (b.len() as u64 <= MAX_PART).then_some(b)
}

/// The contents of the part name.
pub(crate) fn read<R: Read + Seek>(zr: &mut ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    read_file(zr.by_name(name).ok()?)
}

/// The contents of the part name, whatever the case of its name (part
/// names are case-insensitive: Excel writes rdrichvalue.xml, others
/// rdRichValue.xml).
pub(crate) fn read_any_case<R: Read + Seek>(zr: &mut ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    if let Some(b) = read(zr, name) {
        return Some(b);
    }
    let real = zr.file_names().find(|n| n.eq_ignore_ascii_case(name))?.to_string();
    read(zr, &real)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn parts() {
        let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zw.start_file("xl/rdRichValue.xml", zip::write::SimpleFileOptions::default()).unwrap();
        zw.write_all(b"<x/>").unwrap();
        let mut zr = ZipArchive::new(Cursor::new(zw.finish().unwrap().into_inner())).unwrap();
        assert_eq!(read_any_case(&mut zr, "xl/rdrichvalue.xml").as_deref(), Some(&b"<x/>"[..]));
        assert!(read(&mut zr, "xl/rdrichvalue.xml").is_none());
        assert!(read_file(std::io::repeat(0).take(MAX_PART + 1)).is_none());
    }
}
