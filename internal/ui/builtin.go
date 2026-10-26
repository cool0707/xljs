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
// (ui.table) with their scripts and styles.
var BuiltinFS fs.FS

func init() {
	sub, err := fs.Sub(webFiles, "web")
	if err != nil {
		panic(err)
	}
	BuiltinFS = sub
}
