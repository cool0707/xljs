use std::io;

use crate::embed::{self, DAYJS_FILES, WEB_FILES};
use crate::meta::{AssetFs, covered};

/// The folder under ASSET_ORIGIN that serves the built-in pages (forms and
/// tables), e.g. https://app.xljs/_xljs/form.html.
pub const BUILTIN_PREFIX: &str = "_xljs/";

/// The page runtime (web/xljs.js: the xljs object, data-bind and friends)
/// that windows run in every page of the script before the page's own
/// scripts.
pub fn page_script() -> &'static str {
    builtin_text("xljs.js")
}

/// A built-in text file that is always there.
fn builtin_text(name: &str) -> &'static str {
    embed::find(&WEB_FILES, name)
        .unwrap_or_else(|| panic!("web/{name} is missing"))
        .text()
}

/// The height of the custom title bar in logical pixels
/// (--xljs-titlebar-height in web/titlebar.js).
pub const TITLE_BAR_HEIGHT: i64 = 40;

/// The script that puts the custom title bar into every page of a window
/// with WindowOptions::custom_title_bar. non_client tells whether the
/// WebView supports CSS app-region (dragging the window by the bar).
pub fn title_bar_script(o: &super::WindowOptions, non_client: bool) -> String {
    let cfg = serde_json::json!({
        "title": o.title,
        "icon": o.title_icon,
        "fixedSize": o.fixed_size,
        "nonClient": non_client,
        "texts": {
            "minimize": crate::t!("window.minimize"),
            "maximize": crate::t!("window.maximize"),
            "restore": crate::t!("window.restore"),
            "close": crate::t!("window.close"),
        },
    });
    format!(
        "window.__xljsTitleBar = {cfg};\n{}",
        builtin_text("titlebar.js")
    )
}

/// A built-in file (form.html, table.html, their scripts and styles, Font
/// Awesome Free, the debugger page) by its path under web/.
pub fn builtin_file(name: &str) -> Option<&'static [u8]> {
    embed::find(&WEB_FILES, name).map(embed::File::bytes)
}

/// The built-in files as an AssetFs (paths relative to web/).
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinAssets;

impl AssetFs for BuiltinAssets {
    fn read(&self, name: &str) -> io::Result<Vec<u8>> {
        builtin_file(name)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, name.to_string()))
    }
    fn files(&self, root: &str) -> io::Result<Vec<String>> {
        let roots = [root.to_string()];
        Ok(WEB_FILES
            .iter()
            .map(|f| f.name.to_string())
            .filter(|n| covered(&roots, n))
            .collect())
    }
}

/// The Day.js files (dayjs.min.js, locale/*.js, plugin/*.js).
pub(crate) fn dayjs_files() -> &'static [embed::File] {
    &DAYJS_FILES
}
