// Package engine runs xljs scripts: it bundles the entry file with esbuild,
// sets up the goja runtime with the xljs API and drives the event loop.
package engine

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/cool0707/xljs/internal/api"
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
	code := o.Code
	if code == nil {
		var err error
		code, err = Bundle(o.ScriptPath)
		if err != nil {
			return 1, err
		}
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

	script := o.ScriptPath
	if abs, err := filepath.Abs(script); err == nil {
		script = abs
	}
	env := &api.Env{
		UI:      o.UI,
		Args:    o.Args,
		Script:  script,
		Version: o.Version,
		Stdout:  o.Stdout,
		Stderr:  o.Stderr,
		Exit:    func(c int) { vm.Interrupt(&exitSignal{c}) },
	}
	defer env.Cleanup()
	if err := api.Install(vm, env); err != nil {
		return 1, fmt.Errorf("internal error: %w", err)
	}

	name := filepath.Base(o.ScriptPath)
	ast, err := goja.Parse(name, string(code))
	if err != nil {
		return 1, &ScriptError{Message: cleanStack(err.Error())}
	}
	prog, err := goja.CompileAST(ast, false)
	if err != nil {
		return 1, &ScriptError{Message: cleanStack(err.Error())}
	}

	var result goja.Value
	mainFn, err := vm.RunProgram(prog)
	if err == nil {
		if fn, ok := goja.AssertFunction(mainFn); ok {
			result, err = fn(goja.Undefined())
		} else {
			err = errors.New("internal error: bundled script is not a function")
		}
	}
	if err == nil {
		err = l.run()
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
			return 1, &ScriptError{Message: "スクリプトが完了しませんでした（解決されない Promise を await しています）"}
		}
	}
	for p := range unhandled {
		return 1, &ScriptError{Message: "Unhandled promise rejection: " + describeValue(p.Result())}
	}
	return 0, nil
}

func describeError(vm *goja.Runtime, err error) string {
	var ex *goja.Exception
	if errors.As(err, &ex) {
		return describeValue(ex.Value())
	}
	return cleanStack(err.Error())
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
			return cleanStack(s)
		}
	}
	return v.String()
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
