// Package engine runs xljs scripts: it bundles the entry file with esbuild,
// sets up the goja runtime with the xljs API and drives the event loop.
package engine

import (
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/cool0707/xljs/internal/api"
	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/meta"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/dop251/goja"
)

// Options configures a script run.
type Options struct {
	// ScriptPath is the entry file. When Code is set it is only used for
	// naming and to derive the script directory.
	ScriptPath string
	// Code is an already bundled script (from Bundle or a packed exe).
	Code []byte
	// Args are the arguments passed to the script (xljs.args).
	Args    []string
	UI      ui.UI
	Stdout  io.Writer
	Stderr  io.Writer
	Version string
	// Meta is the script's metadata block. When nil it is read from
	// ScriptPath (warnings go to Stderr).
	Meta *meta.Meta
	// Assets are the files served to windows. When nil they are read from
	// the script folder (limited to Meta.AssetRoots).
	Assets fs.FS
	// Debug enables the developer tools in windows. DebugScript runs the
	// script in the debugger window instead of goja (see runDebug).
	Debug       bool
	DebugScript bool
	// debugAutoRun (tests) runs the script in the debugger window without
	// waiting for the user and closes the window at the end.
	debugAutoRun bool
}

// ScriptError is a JavaScript error that terminated the script.
type ScriptError struct {
	Message string
}

func (e *ScriptError) Error() string { return e.Message }

type exitSignal struct{ code int }

// Run executes the script and returns its exit code. A non-nil error
// describes why the script failed (the exit code is then non-zero).
func Run(o Options) (int, error) {
	if o.Stdout == nil {
		o.Stdout = os.Stdout
	}
	if o.Stderr == nil {
		o.Stderr = os.Stderr
	}
	if o.UI == nil {
		o.UI = ui.NewConsole()
	}
	if o.Meta == nil {
		if o.Code != nil {
			o.Meta = &meta.Meta{}
		} else {
			m, warnings, err := meta.ParseFile(o.ScriptPath)
			if err != nil {
				return 1, err
			}
			for _, w := range warnings {
				fmt.Fprintln(o.Stderr, w)
			}
			o.Meta = m
		}
	}
	code := o.Code
	if code == nil {
		var err error
		code, err = bundle(o.ScriptPath, o.DebugScript)
		if err != nil {
			return 1, err
		}
	}
	if o.DebugScript {
		return runDebug(o, code)
	}

	vm := goja.New()
	l := newLoop(vm)
	l.install()

	unhandled := map[*goja.Promise]struct{}{}
	vm.SetPromiseRejectionTracker(func(p *goja.Promise, op goja.PromiseRejectionOperation) {
		if op == goja.PromiseRejectionReject {
			unhandled[p] = struct{}{}
		} else {
			delete(unhandled, p)
		}
	})

	script := absPath(o.ScriptPath)
	if o.Assets == nil {
		o.Assets = o.Meta.AssetFS(filepath.Dir(script))
	}
	env := &api.Env{
		UI:         o.UI,
		Args:       o.Args,
		Script:     script,
		Version:    o.Version,
		Stdout:     o.Stdout,
		Stderr:     o.Stderr,
		Exit:       l.requestExit,
		Title:      o.Meta.Title,
		Meta:       o.Meta,
		Assets:     o.Assets,
		Debug:      o.Debug,
		AddPending: l.addPending,
		Post:       l.post,
	}
	defer env.Cleanup()
	if err := api.Install(vm, env); err != nil {
		return 1, i18n.Err("engine.internalError", err)
	}

	name := filepath.Base(o.ScriptPath)
	ast, err := goja.Parse(name, string(code))
	if err != nil {
		return 1, &ScriptError{Message: i18n.Text(cleanStack(err.Error()))}
	}
	prog, err := goja.CompileAST(ast, false)
	if err != nil {
		return 1, &ScriptError{Message: i18n.Text(cleanStack(err.Error()))}
	}

	// An app opens its main window before the script runs; the script ends
	// when the window is closed (the window keeps the event loop busy).
	if o.Meta.IsApp() {
		if err := api.OpenMainWindow(vm, env); err != nil {
			return 1, &ScriptError{Message: describeError(vm, err)}
		}
	}

	var result goja.Value
	mainFn, err := vm.RunProgram(prog)
	if err == nil {
		if fn, ok := goja.AssertFunction(mainFn); ok {
			result, err = fn(goja.Undefined(), api.ExportsHook(env))
		} else {
			err = i18n.Err("engine.bundleNotFunction")
		}
	}
	if err == nil {
		err = l.run()
	}
	// exit() inside a promise job that Go started (a window closing, a
	// message) does not surface as an error, so check for it explicitly.
	if l.exit != nil {
		vm.ClearInterrupt()
		return l.exit.code, nil
	}
	if err != nil {
		var ie *goja.InterruptedError
		if errors.As(err, &ie) {
			if sig, ok := ie.Value().(*exitSignal); ok {
				return sig.code, nil
			}
		}
		return 1, &ScriptError{Message: describeError(vm, err)}
	}

	if p, ok := result.Export().(*goja.Promise); result != nil && ok {
		delete(unhandled, p)
		switch p.State() {
		case goja.PromiseStateRejected:
			return 1, &ScriptError{Message: describeValue(p.Result())}
		case goja.PromiseStatePending:
			return 1, &ScriptError{Message: i18n.T("engine.notFinished")}
		}
	}
	for p := range unhandled {
		return 1, &ScriptError{Message: i18n.T("engine.unhandledRejection", describeValue(p.Result()))}
	}
	return 0, nil
}

func absPath(p string) string {
	if abs, err := filepath.Abs(p); err == nil {
		return abs
	}
	return p
}

func describeError(vm *goja.Runtime, err error) string {
	var ex *goja.Exception
	if errors.As(err, &ex) {
		return describeValue(ex.Value())
	}
	return i18n.Text(cleanStack(err.Error()))
}

func describeValue(v goja.Value) string {
	if v == nil || goja.IsUndefined(v) {
		return "undefined"
	}
	if obj, ok := v.(*goja.Object); ok {
		if stack := obj.Get("stack"); stack != nil && !goja.IsUndefined(stack) && stack.String() != "" {
			s := stack.String()
			// Some engines omit the message from the stack.
			msg := obj.Get("message")
			if msg != nil && !goja.IsUndefined(msg) && !strings.Contains(s, msg.String()) {
				s = obj.String() + "\n" + s
			}
			return i18n.Text(cleanStack(s))
		}
	}
	return i18n.Text(v.String())
}

var (
	pcSuffix     = regexp.MustCompile(`\((\d+)\)\)`)
	pcSuffixBare = regexp.MustCompile(`(:\d+:\d+)\(\d+\)`)
	nativeFrame  = regexp.MustCompile(`(?m)^\s*at .*\(native\)\s*$\n?`)
	preludeFrame = regexp.MustCompile(`(?m)^\s*at .*xljs:prelude.*$\n?`)
)

// cleanStack removes goja's program-counter annotations and internal
// frames so the stack trace only shows the user's files.
func cleanStack(s string) string {
	s = pcSuffix.ReplaceAllString(s, ")")
	s = pcSuffixBare.ReplaceAllString(s, "$1")
	s = nativeFrame.ReplaceAllString(s, "")
	s = preludeFrame.ReplaceAllString(s, "")
	return strings.TrimRight(s, "\n")
}
