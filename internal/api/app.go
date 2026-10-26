package api

import (
	"io/fs"
	"net/url"
	"path"
	"strings"
	"sync/atomic"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/dop251/goja"
)

// pageURL returns the URL of an HTML file given relative to the script
// folder ("" for an empty page) and its name in the assets.
func pageURL(html string) (string, string, error) {
	if html == "" {
		return "", "", nil
	}
	p := path.Clean(strings.ReplaceAll(html, `\`, "/"))
	if path.IsAbs(p) || p == ".." || strings.HasPrefix(p, "../") || strings.Contains(p, ":") {
		return "", "", i18n.Err("app.badHTML", html)
	}
	u := url.URL{Path: p}
	return ui.AssetOrigin + u.EscapedPath(), p, nil
}

// windowAssets serves the built-in pages (ui.BuiltinFS) under
// ui.BuiltinPrefix and the script's assets everywhere else.
type windowAssets struct{ app fs.FS }

func (a windowAssets) Open(name string) (fs.File, error) {
	dir := strings.TrimSuffix(ui.BuiltinPrefix, "/")
	if name == dir {
		return ui.BuiltinFS.Open(".")
	}
	if rest, ok := strings.CutPrefix(name, ui.BuiltinPrefix); ok {
		return ui.BuiltinFS.Open(rest)
	}
	if a.app == nil {
		return nil, &fs.PathError{Op: "open", Path: name, Err: fs.ErrNotExist}
	}
	return a.app.Open(name)
}

// cancelMessage is the message a page sends to ask a running task to stop.
// It is noted as soon as it arrives, so that a script busy in a loop can
// see it (cancelRequested) before the message itself is delivered.
const cancelMessage = `{"type":"cancel"}`

func (n *natives) installApp() {
	vm, env := n.vm, n.env
	if env.Meta != nil {
		n.obj.Set("appMode", env.Meta.IsApp())
		n.obj.Set("appHTML", env.Meta.HTML)
		n.obj.Set("appWidth", env.Meta.Width)
		n.obj.Set("appHeight", env.Meta.Height)
	}
	assets := windowAssets{env.Assets}
	n.obj.Set("supportsWindows", env.UI.SupportsWindows())
	// appOpen(html, title, width, height, onMessage(msg, files)) ->
	// {close(), closed: Promise, post(msg), isClosed(), cancelRequested(), clearCancel()}
	n.set("appOpen", func(c goja.FunctionCall) goja.Value {
		html := n.str(c, 0)
		u, name, err := pageURL(html)
		if err != nil {
			n.throw(err)
		}
		if name != "" {
			if _, err := fs.Stat(assets, name); err != nil {
				n.throwT("app.htmlNotFound", html)
			}
		}
		onMessage, _ := goja.AssertFunction(c.Argument(4))
		var isClosed, cancel atomic.Bool
		closed, resolve, _ := vm.NewPromise()
		// The open window keeps the event loop (and the process) alive.
		env.AddPending()
		w, err := env.UI.OpenWindow(ui.WindowOptions{
			Title:  n.optStr(c.Argument(1), env.Title),
			Width:  int(c.Argument(2).ToInteger()),
			Height: int(c.Argument(3).ToInteger()),
			URL:    u,
			Assets: assets,
			Debug:  env.Debug,
			OnMessage: func(msg string, files []string) {
				if msg == cancelMessage {
					cancel.Store(true)
				}
				if onMessage == nil {
					return
				}
				env.AddPending()
				env.Post(func() error {
					_, err := onMessage(goja.Undefined(), vm.ToValue(msg), n.strArray(files))
					return err
				})
			},
			OnClosed: func() {
				isClosed.Store(true)
				env.Post(func() error {
					resolve(goja.Undefined())
					return nil
				})
			},
		})
		if err != nil {
			env.Post(func() error { return nil })
			n.throw(err)
		}
		env.OnCleanup(w.Close)
		obj := vm.NewObject()
		obj.Set("close", func(goja.FunctionCall) goja.Value {
			w.Close()
			return goja.Undefined()
		})
		obj.Set("closed", closed)
		obj.Set("post", func(c goja.FunctionCall) goja.Value {
			w.Post(n.str(c, 0))
			return goja.Undefined()
		})
		obj.Set("isClosed", func(goja.FunctionCall) goja.Value { return vm.ToValue(isClosed.Load()) })
		obj.Set("cancelRequested", func(goja.FunctionCall) goja.Value { return vm.ToValue(cancel.Load()) })
		obj.Set("clearCancel", func(goja.FunctionCall) goja.Value {
			cancel.Store(false)
			return goja.Undefined()
		})
		return obj
	})
}

// OpenMainWindow opens the main window of an app-mode script (see @mode).
func OpenMainWindow(vm *goja.Runtime, env *Env) error {
	fn, ok := goja.AssertFunction(env.natives.obj.Get("openMain"))
	if !ok {
		return i18n.Err("engine.internalError", "openMain")
	}
	_, err := fn(goja.Undefined())
	return err
}
