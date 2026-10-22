package main

import (
	"bufio"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/cool0707/xljs/internal/engine"
	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"golang.org/x/term"
)

// consoleState tracks how the process is attached to a console.
type consoleState struct {
	// has is true when stdout/stderr reach a console or a redirect.
	has bool
	// own is true when the console window was created for this process
	// (e.g. the exe was double-clicked) and closes when it exits.
	own bool
}

func (c *consoleState) finish() {
	if c.own && term.IsTerminal(int(os.Stdin.Fd())) {
		fmt.Fprint(os.Stderr, "\n"+i18n.S("Enter キーで閉じます...", "Press Enter to close..."))
		bufio.NewReader(os.Stdin).ReadString('\n')
	}
}

// fail reports err and returns the exit code 1. Without a console the error
// is shown in a dialog so that GUI builds never fail silently.
func (c *consoleState) fail(u ui.UI, err error) int {
	msg := err.Error()
	var be *engine.BundleError
	var se *engine.ScriptError
	switch {
	case errors.As(err, &be):
		msg = i18n.S("構文エラー:\n", "Syntax error:\n") + msg
	case errors.As(err, &se):
		// already formatted and localized
	default:
		msg = i18n.S("エラー: ", "Error: ") + i18n.Error(err)
	}
	fmt.Fprintln(os.Stderr, msg)
	if !c.has {
		if u == nil && ui.NativeAvailable() {
			u, _ = ui.NewNative()
		}
		if u != nil && u.Name() == "native" {
			title := "xljs"
			if exe, e := os.Executable(); e == nil {
				title = strings.TrimSuffix(filepath.Base(exe), filepath.Ext(exe))
			}
			u.ShowError(title, msg)
		}
	}
	return 1
}
