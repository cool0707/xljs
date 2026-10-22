// Package webui holds the built-in pages shown in xljs windows: the form
// of ui.form and the table of ui.table. They talk to the script through
// the page bridge (window.xljs, see internal/ui/bridge.js).
package webui

import "embed"

// FS contains form.html, table.html and their scripts and styles.
//
//go:embed *.html *.js *.css
var FS embed.FS
