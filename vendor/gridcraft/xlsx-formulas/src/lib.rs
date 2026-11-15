//! How formulas are written in xlsx files, from GridCraft's `gridcraft-xlsx` crate: the `_xlfn.`
//! and `_xlpm.` prefixes ([`fmla`]), the functions that take them ([`tables`]) and the reading of
//! legacy formulas, which Excel evaluates with implicit intersection (`array_formula`).
//!
//! This file is xljs's: the three modules are GridCraft's as they are, and what xljs calls of
//! their crate-private parts is exported here.
#![forbid(unsafe_code)]
// The writer's half of `array_formula` (which formulas to save as dynamic arrays) has no caller here.
#![allow(dead_code)]

use gridcraft_core::CellRef;
use gridcraft_model::Workbook;

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
