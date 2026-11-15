//! xljs: stands in for the `gridcraft-math` crate (LaTeX equation layout with RaTeX), which is
//! not vendored: no equation is laid out here, and the calculation engine never asks for one.
//! [`EquationCache`](crate::EquationCache) keeps its shape, so the rest of the model is unchanged.

use std::ops::Range;

/// Longest equation source, in bytes.
pub const MAX_SOURCE_BYTES: usize = 4096;

/// The error for a source over [`MAX_SOURCE_BYTES`].
pub static SOURCE_TOO_LONG: std::sync::LazyLock<MathError> =
    std::sync::LazyLock::new(|| MathError { span: None, message: format!("An equation can be at most {MAX_SOURCE_BYTES} bytes long.") });

/// Why an equation could not be laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathError {
    pub span: Option<Range<usize>>,
    pub message: String,
}

/// A laid-out equation (never built here).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MathList {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

/// Always an error: equations are not laid out in this build.
pub fn layout(latex: &str) -> Result<MathList, MathError> {
    if latex.len() > MAX_SOURCE_BYTES {
        return Err(SOURCE_TOO_LONG.clone());
    }
    Err(MathError { span: None, message: "Equations are not laid out in this build.".to_string() })
}
