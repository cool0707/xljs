//! How formulas are written in xlsx files, from GridCraft's `gridcraft-xlsx` crate: the `_xlfn.`
//! and `_xlpm.` prefixes ([`fmla`]), the functions that take them ([`tables`]) and the reading of
//! legacy formulas, which Excel evaluates with implicit intersection (`array_formula`).
//!
//! This file is xljs's: the three modules are GridCraft's as they are, and what xljs calls of
//! their crate-private parts is exported here ([`apply_legacy`] for the formulas of a file,
//! [`save_as`] and [`legacy_reading`] for the formulas scripts write and read).
#![forbid(unsafe_code)]
// What the modules have for GridCraft's own reader (Google Sheets' dummy functions) has no caller here.
#![allow(dead_code)]

use gridcraft_core::CellRef;
use gridcraft_formula::Expr;
use gridcraft_model::{Formula, Workbook};

mod array_formula;
pub mod fmla;
pub mod tables;

/// What the legacy-formula analysis needs to know about worksheet functions (from
/// `crates/xlsx/src/lib.rs` of GridCraft).
#[derive(Clone, Copy, Debug)]
pub struct FunctionInfo {
    /// Whether `name` (upper case, without `_xlfn.`) is a built-in worksheet function rather than
    /// an add-in, VBA or LAMBDA function.
    pub is_builtin: fn(&str) -> bool,
    /// Whether parameter `i` of the built-in function `name` takes a single value, as opposed to
    /// a reference or array taken as a whole.
    pub is_scalar_arg: fn(&str, usize) -> bool,
}

impl Default for FunctionInfo {
    /// Without the function library: every name counts as built in, and this crate's table of
    /// reference- and array-taking parameters alone decides parameter types.
    fn default() -> Self {
        FunctionInfo { is_builtin: |_| true, is_scalar_arg: |_, _| true }
    }
}

/// Rewrites the legacy cell formulas at `cells` (sheet index, positions) to their dynamic-array
/// reading, with `@` where Excel intersects: a formula saved without dynamic-array metadata shows
/// one value, and the evaluator gives that value only with the `@` written out. Call it once the
/// workbook's names and tables are in place.
pub fn apply_legacy(wb: &mut Workbook, cells: Vec<(usize, Vec<CellRef>)>, functions: FunctionInfo) {
    array_formula::apply_legacy(wb, cells, functions);
}

/// The defined names analysed so far for [`save_as`] and [`legacy_reading`]: each is analysed once
/// for all the formulas that refer to it. It holds as long as the workbook's names and tables do
/// not change.
#[derive(Default)]
pub struct Names(array_formula::NameCache);

/// How a formula entered as dynamic-array Excel reads it is written to a file.
pub enum SaveAs {
    /// As it is, without dynamic-array metadata: it gives one value either way.
    Unchanged,
    /// As this expression (the formula without the `@` legacy evaluation implies), without
    /// dynamic-array metadata.
    Legacy(Expr),
    /// As an array formula with dynamic-array metadata (`cm`): legacy evaluation would differ.
    Dynamic,
}

/// How the formula of the cell `at` is saved (the writer's half of `array_formula`).
pub fn save_as(wb: &Workbook, sheet: usize, at: CellRef, formula: &Formula, functions: FunctionInfo, names: &mut Names) -> SaveAs {
    match array_formula::export(wb, sheet, at, formula, functions, &mut names.0) {
        array_formula::Export::Unchanged => SaveAs::Unchanged,
        array_formula::Export::Legacy(e) => SaveAs::Legacy(e),
        array_formula::Export::Dynamic => SaveAs::Dynamic,
    }
}

/// The dynamic-array reading of a formula saved without dynamic-array metadata, as Excel shows
/// it (with `@` where it intersects), when it differs from the formula as saved.
pub fn legacy_reading(wb: &Workbook, sheet: usize, at: CellRef, formula: &Formula, functions: FunctionInfo, names: &mut Names) -> Option<Formula> {
    array_formula::legacy(wb, sheet, at, formula, functions, &mut names.0)
}
