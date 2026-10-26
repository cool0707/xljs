// Package ui defines the user-interaction layer used by scripts (dialogs,
// file pickers, progress) and its implementations: native Windows dialogs,
// a console fallback, and a scripted mock for automated tests.
package ui

import (
	"io/fs"
	"sync/atomic"
)

// FileFilter is one entry of a file type filter in a picker.
type FileFilter struct {
	Description string
	Patterns    []string // e.g. "*.xlsx"
}

// PickerOptions configures file and directory pickers.
type PickerOptions struct {
	Title         string
	Multiple      bool
	Filters       []FileFilter
	StartIn       string // directory to start in ("" = default)
	SuggestedName string // save picker only
}

// ProgressOptions configures a progress indicator.
type ProgressOptions struct {
	Title       string
	Total       int // 0 = indeterminate
	Message     string
	Cancellable bool
}

// Progress is a live progress indicator.
type Progress interface {
	// Update sets the number of completed items, the total (0 keeps the
	// current total) and the message line.
	Update(done, total int, message string)
	// Cancelled reports whether the user asked to cancel.
	Cancelled() bool
	// Close removes the indicator. It is safe to call more than once.
	Close()
}

// AssetOrigin is the URL under which a window serves WindowOptions.Assets.
const AssetOrigin = "https://app.xljs/"

// WindowOptions configures an HTML window.
type WindowOptions struct {
	Title string
	// Width and Height are the client size in logical pixels (0 = default).
	Width, Height int
	// URL is the first page; "" shows an empty page. Pages under
	// AssetOrigin are served from Assets.
	URL    string
	Assets fs.FS
	// Debug enables the developer tools (F12) and the context menu;
	// DevTools also opens them with the window.
	Debug    bool
	DevTools bool
	// Isolated serves the pages cross-origin isolated (COOP and COEP
	// headers), so that they can use SharedArrayBuffer and Atomics.wait
	// (the debugger window, see internal/ui/web/debug-page.js).
	Isolated bool
	// Icon is an .ico file or a PNG image for the title bar and taskbar
	// (nil = the exe's icon). See CheckIcon.
	Icon []byte
	// OnMessage receives the messages posted by the page, with the paths of
	// the files the page attached (chrome.webview.postMessageWithAdditionalObjects).
	// OnClosed is called once after the window has closed (by the user or
	// Close). Both may be called on any goroutine and must not block.
	OnMessage func(msg string, files []string)
	OnClosed  func()
}

// WebView2DataEnv names the environment variable that overrides the
// WebView2 user data folder (default %LOCALAPPDATA%\xljs\WebView2). Test
// binaries running at the same time (go test ./...) each set their own.
const WebView2DataEnv = "XLJS_WEBVIEW2_DATA"

// Window is an open HTML window.
type Window interface {
	// Post sends a message to the page.
	Post(msg string)
	// Close closes the window. It is safe to call more than once.
	Close()
}

// UI is the set of interactions available to scripts. Implementations are
// called from one thread only (see Run).
type UI interface {
	Name() string
	Alert(title, message string)
	Confirm(title, message string) bool
	// Prompt returns the entered text and false when cancelled.
	Prompt(title, message, def string) (string, bool)
	// OpenFiles returns the selected paths (empty when cancelled).
	OpenFiles(opts PickerOptions) []string
	// SaveFile returns the chosen path and false when cancelled.
	SaveFile(opts PickerOptions) (string, bool)
	// PickDirectory returns the chosen directory and false when cancelled.
	PickDirectory(opts PickerOptions) (string, bool)
	Progress(opts ProgressOptions) Progress
	// ShowError reports a fatal script error to the user.
	ShowError(title, message string)
	// ShowInFolder opens the file manager with the given files selected
	// (files in different folders open one window per folder).
	ShowInFolder(paths []string) error
	// OpenPath opens a file, folder or URL with its default application.
	OpenPath(target string) error
	// OpenWindow opens an HTML window (WebView2). It fails when the UI
	// cannot show one (console) or the WebView2 runtime is missing.
	OpenWindow(o WindowOptions) (Window, error)
	// SupportsWindows reports whether forms and tables are shown as HTML
	// windows (driven by the script through OpenWindow) instead of Form and
	// Table.
	SupportsWindows() bool
	// Form asks for the values of a form and returns them by field name, or
	// false when cancelled. See FormOptions for the value types.
	Form(o FormOptions) (map[string]any, bool)
	// Table shows rows. With o.Select it returns the indices of the chosen
	// rows, or false when cancelled.
	Table(o TableOptions) ([]int, bool)
}

// FormOptions describes a form for UIs without HTML windows. Values are
// strings for every field type except checkbox (bool), files ([]string, one
// path each) and table (the indices of the selected rows); empty means no
// value.
type FormOptions struct {
	Title       string      `json:"title"`
	Description string      `json:"description"`
	Fields      []FormField `json:"fields"`
	// Error is a message about the form as a whole (from a previous attempt).
	Error string `json:"error"`
	// Submit is the label of the submit button ("" = none) and Buttons
	// those of the other buttons. With buttons, the values returned name
	// the one pressed in "$button": its index, or nil for submit.
	Submit  string   `json:"submit"`
	Buttons []string `json:"buttons"`
}

// FormField is one input of a form.
type FormField struct {
	Name        string       `json:"name"`
	Label       string       `json:"label"`
	Type        string       `json:"type"` // text, password, number, date, month, time, select, radio, checkbox, textarea, file, files, folder, saveFile, table
	Options     []FormOption `json:"options"`
	Required    bool         `json:"required"`
	Placeholder string       `json:"placeholder"`
	Help        string       `json:"help"`
	Accept      []string     `json:"accept"` // file types: ".xlsx"
	Value       any          `json:"value"`  // current value
	Error       string       `json:"error"`  // message from a previous attempt
	// Tab is the label of the tab the field is on ("" = above the tabs).
	Tab string `json:"tab"`
	// Columns, Rows and Select describe a table field (see TableOptions);
	// a table without Select only shows rows.
	Columns []string   `json:"columns"`
	Rows    [][]string `json:"rows"`
	Select  string     `json:"select"`
}

// FormOption is a choice of a select or radio field.
type FormOption struct {
	Value string `json:"value"`
	Label string `json:"label"`
}

// Selection modes of a table.
const (
	SelectNone     = ""
	SelectSingle   = "single"
	SelectMultiple = "multiple"
)

// TableOptions describes a table for UIs without HTML windows. Cells are
// already formatted as text.
type TableOptions struct {
	Title   string     `json:"title"`
	Columns []string   `json:"columns"`
	Rows    [][]string `json:"rows"`
	Select  string     `json:"select"`
}

// interrupts counts Ctrl+C presses so that progress indicators can report
// cancellation from the console as well.
var interrupts atomic.Int32

// NotifyInterrupt records a Ctrl+C press. It returns the total number of
// presses so far.
func NotifyInterrupt() int32 { return interrupts.Add(1) }

// Interrupted reports whether Ctrl+C was pressed at least once.
func Interrupted() bool { return interrupts.Load() > 0 }

// activeProgress counts open progress indicators; while one is open the
// first Ctrl+C only requests cancellation.
var activeProgress atomic.Int32

// ProgressActive reports whether any progress indicator is open.
func ProgressActive() bool { return activeProgress.Load() > 0 }
