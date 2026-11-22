//! A small XML tree (local names only) for the parts of a workbook that are
//! read whole: the workbook, the styles, drawings, charts, the rules of a
//! sheet. Also the reading of a subtree while the rest of a sheet is
//! streamed (src/book).
//!
//! The tree is umya-spreadsheet's (`xml_tree`, in the fork; RAS reads
//! workbooks with it too): elements nested deeper than 256 are left out,
//! `parse` reads well-formed documents only and `parse_partial` what can be
//! read. `element` and `subtree` take the types of quick-xml 0.41 (the
//! version umya-spreadsheet uses).

pub(crate) use umya_spreadsheet::xml_tree::{Node, element, parse, parse_partial, subtree};
