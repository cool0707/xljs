//! xljs: runs JavaScript/TypeScript scripts that read and write Excel, CSV
//! and JSON files, with file pickers, dialogs, progress indicators and HTML
//! windows (WebView2). See README.md.

pub mod ad;
pub mod api;
pub mod book;
pub mod cli;
pub mod db;
pub mod dev;
pub(crate) mod embed;
pub mod engine;
pub mod pdf;
mod error;
pub mod i18n;
pub mod meta;
pub mod net;
pub mod pack;
pub mod process;
pub mod shortcut;
pub mod ui;
pub mod vba;
pub(crate) mod xml;

pub use error::{Error, Result};

/// The type definitions of the public API (`xljs init`).
pub fn dts() -> &'static str {
    embed::DTS.text()
}

#[cfg(test)]
pub(crate) mod testutil;
#[cfg(test)]
pub(crate) use testutil::lock_lang as test_lang;
