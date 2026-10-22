package api

import (
	"io/fs"
	"net/url"
	"path"
	"strings"

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

func (n *natives) installApp() {
	vm, env := n.vm, n.env
	if env.Meta != nil {
		n.obj.Set("appMode", env.Meta.IsApp())
		n.obj.Set("appHTML", env.Meta.HTML)
		n.obj.Set("appWidth", env.Meta.Width)
		n.obj.Set("appHeight", env.Meta.Height)
	}
	// appOpen(html, title, width, height) -> {close(), closed: Promise}
	n.set("appOpen", func(c goja.FunctionCall) goja.Value {
		html := n.str(c, 0)
		u, name, err := pageURL(html)
		if err != nil {
			n.throw(err)
		}
		if name != "" {
			if _, err := fs.Stat(env.Assets, name); err != nil {
				n.throwT("app.htmlNotFound", html)
			}
		}
		closed, resolve, _ := vm.NewPromise()
		// The open window keeps the event loop (and the process) alive.
		env.AddPending()
		w, err := env.UI.OpenWindow(ui.WindowOptions{
			Title:  n.optStr(c.Argument(1), env.Title),
			Width:  int(c.Argument(2).ToInteger()),
			Height: int(c.Argument(3).ToInteger()),
			URL:    u,
			Assets: env.Assets,
			Debug:  env.Debug,
			OnClosed: func() {
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
