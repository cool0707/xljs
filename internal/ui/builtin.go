package ui

import (
	"embed"
	"io/fs"
)

//go:embed web
var webFiles embed.FS

// BuiltinPrefix is the folder under AssetOrigin that serves BuiltinFS
// (forms and tables), e.g. https://app.xljs/_xljs/form.html.
const BuiltinPrefix = "_xljs/"

// BuiltinFS holds the built-in pages: form.html (ui.form) and table.html
// (ui.table) with their scripts and styles, and Font Awesome Free
// (fontawesome/css/all.min.css) for the script's own pages.
var BuiltinFS fs.FS

// PageScript is the page runtime (the xljs object, data-bind and friends)
// that windows run in every page of the script before the page's own
// scripts.
//
//go:embed web/xljs.js
var PageScript string

func init() {
	sub, err := fs.Sub(webFiles, "web")
	if err != nil {
		panic(err)
	}
	BuiltinFS = sub
}
