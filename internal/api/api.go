// Package api implements the JavaScript surface of xljs: the native
// functions written in Go and the prelude that builds the public objects
// (fs, path, csv, workbooks, pickers, ui, console, polyfills) on top of them.
package api

import (
	_ "embed"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/meta"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/dop251/goja"
)

// Env carries the host environment into the API.
type Env struct {
	UI      ui.UI
	Args    []string
	Script  string // absolute path of the script (or the packed exe)
	Version string
	Stdout  io.Writer
	Stderr  io.Writer
	Exit    func(code int)
	// Title is the default title of dialogs and windows.
	Title string
	// Meta is the script's metadata block; Assets its files (served to
	// windows under ui.AssetOrigin). Debug enables the developer tools.
	Meta   *meta.Meta
	Assets fs.FS
	Debug  bool
	// AddPending keeps the event loop running until a matching Post; Post
	// runs fn on the script thread and may be called from any goroutine.
	AddPending func()
	Post       func(fn func() error)

	cleanup []func()
	natives *natives
}

// OnCleanup registers fn to run when the script ends.
func (e *Env) OnCleanup(fn func()) { e.cleanup = append(e.cleanup, fn) }

// Cleanup closes resources left open by the script (progress indicators,
// workbooks). It is safe to call more than once.
func (e *Env) Cleanup() {
	for i := len(e.cleanup) - 1; i >= 0; i-- {
		e.cleanup[i]()
	}
	e.cleanup = nil
}

//go:embed prelude.js
var preludeSrc string

var (
	preludeOnce sync.Once
	preludeProg *goja.Program
	preludeErr  error
)

// Install defines the xljs globals in vm.
func Install(vm *goja.Runtime, env *Env) error {
	preludeOnce.Do(func() {
		preludeProg, preludeErr = goja.Compile("xljs:prelude", preludeSrc, true)
	})
	if preludeErr != nil {
		return preludeErr
	}
	n := &natives{vm: vm, env: env, obj: vm.NewObject()}
	env.natives = n
	if env.Title == "" {
		env.Title = strings.TrimSuffix(filepath.Base(env.Script), filepath.Ext(env.Script))
	}
	n.installCore()
	n.installFS()
	n.installEncoding()
	n.installCSV()
	n.installXLSX()
	n.installXLSXStyle()
	n.installUI()
	n.installApp()

	fnv, err := vm.RunProgram(preludeProg)
	if err != nil {
		return err
	}
	fn, ok := goja.AssertFunction(fnv)
	if !ok {
		return errors.New("prelude did not evaluate to a function")
	}
	_, err = fn(goja.Undefined(), n.obj)
	return err
}

type natives struct {
	vm  *goja.Runtime
	env *Env
	obj *goja.Object
}

func (n *natives) set(name string, fn func(goja.FunctionCall) goja.Value) {
	if err := n.obj.Set(name, fn); err != nil {
		panic(err)
	}
}

// throw raises err as a JavaScript Error with a localized message.
func (n *natives) throw(err error) {
	ctor, _ := goja.AssertConstructor(n.vm.Get("Error"))
	if ctor != nil {
		if obj, e := ctor(nil, n.vm.ToValue(i18n.Error(err))); e == nil {
			panic(obj)
		}
	}
	panic(n.vm.NewGoError(err))
}

// throwT raises the localized message for key.
func (n *natives) throwT(key string, args ...any) { n.throw(i18n.Err(key, args...)) }

// throwPath raises err for the file at path, adding the path unless the
// error already names it.
func (n *natives) throwPath(path string, err error) {
	var pe *fs.PathError
	if errors.As(err, &pe) {
		n.throw(err)
	}
	n.throw(fmt.Errorf("%s: %w", path, err))
}

func (n *natives) str(call goja.FunctionCall, i int) string {
	v := call.Argument(i)
	if goja.IsUndefined(v) || goja.IsNull(v) {
		return ""
	}
	return v.String()
}

func (n *natives) optStr(v goja.Value, def string) string {
	if v == nil || goja.IsUndefined(v) || goja.IsNull(v) {
		return def
	}
	return v.String()
}

func isNullish(v goja.Value) bool { return v == nil || goja.IsUndefined(v) || goja.IsNull(v) }

// elems returns the elements of an array-like value.
func (n *natives) elems(v goja.Value) []goja.Value {
	if isNullish(v) {
		return nil
	}
	obj := v.ToObject(n.vm)
	l := int(obj.Get("length").ToInteger())
	out := make([]goja.Value, l)
	for i := 0; i < l; i++ {
		out[i] = obj.Get(strconv.Itoa(i))
	}
	return out
}

func (n *natives) strings(v goja.Value) []string {
	if isNullish(v) {
		return nil
	}
	if _, ok := v.Export().(string); ok {
		return []string{v.String()}
	}
	es := n.elems(v)
	out := make([]string, 0, len(es))
	for _, e := range es {
		if !isNullish(e) {
			out = append(out, e.String())
		}
	}
	return out
}

// bytes converts an ArrayBuffer, typed array or string to bytes.
func (n *natives) bytes(v goja.Value) []byte {
	if isNullish(v) {
		return nil
	}
	switch x := v.Export().(type) {
	case goja.ArrayBuffer:
		return x.Bytes()
	case []byte:
		return x
	case string:
		return []byte(x)
	}
	if obj, ok := v.(*goja.Object); ok {
		// Typed arrays other than Uint8Array: use the underlying buffer.
		if buf := obj.Get("buffer"); buf != nil {
			if ab, ok := buf.Export().(goja.ArrayBuffer); ok {
				off := int(obj.Get("byteOffset").ToInteger())
				l := int(obj.Get("byteLength").ToInteger())
				return ab.Bytes()[off : off+l]
			}
		}
	}
	n.throwT("data.expectedBytes")
	return nil
}

func (n *natives) installCore() {
	vm, env := n.vm, n.env
	n.set("stdout", func(c goja.FunctionCall) goja.Value {
		io.WriteString(env.Stdout, n.str(c, 0))
		return goja.Undefined()
	})
	n.set("stderr", func(c goja.FunctionCall) goja.Value {
		io.WriteString(env.Stderr, n.str(c, 0))
		return goja.Undefined()
	})
	n.set("exit", func(c goja.FunctionCall) goja.Value {
		code := 0
		if !isNullish(c.Argument(0)) {
			code = int(c.Argument(0).ToInteger())
		}
		env.Exit(code)
		return goja.Undefined()
	})
	n.set("getenv", func(c goja.FunctionCall) goja.Value {
		v, ok := os.LookupEnv(n.str(c, 0))
		if !ok {
			return goja.Null()
		}
		return vm.ToValue(v)
	})
	n.obj.Set("args", n.strArray(env.Args))
	n.obj.Set("script", env.Script)
	n.obj.Set("version", env.Version)
	n.obj.Set("platform", runtime.GOOS)
	n.obj.Set("uiName", env.UI.Name())
	n.obj.Set("lang", i18n.Lang())
	n.set("t", func(c goja.FunctionCall) goja.Value {
		args := make([]any, 0, len(c.Arguments))
		for _, a := range c.Arguments[min(1, len(c.Arguments)):] {
			args = append(args, a.String())
		}
		return n.vm.ToValue(i18n.T(n.str(c, 0), args...))
	})
	n.obj.Set("title", env.Title)
}

// array builds a real JavaScript array.
func (n *natives) array(vals []goja.Value) goja.Value {
	items := make([]any, len(vals))
	for i, v := range vals {
		items[i] = v
	}
	return n.vm.NewArray(items...)
}

func (n *natives) strArray(ss []string) goja.Value {
	items := make([]any, len(ss))
	for i, s := range ss {
		items[i] = s
	}
	return n.vm.NewArray(items...)
}

// object builds a plain JavaScript object.
func (n *natives) object(m map[string]any) goja.Value {
	o := n.vm.NewObject()
	for k, v := range m {
		o.Set(k, v)
	}
	return o
}
