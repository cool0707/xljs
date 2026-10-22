// Package ui defines the user-interaction layer used by scripts (dialogs,
// file pickers, progress) and its implementations: native Windows dialogs,
// a console fallback, and a scripted mock for automated tests.
package ui

import (
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

// UI is the set of interactions available to scripts. Implementations are
// called from the script thread only.
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
