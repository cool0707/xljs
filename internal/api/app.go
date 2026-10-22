package api

import (
	"encoding/json"
	"io/fs"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/cool0707/xljs/internal/webui"
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
	// appOpen({html, page, title, width, height, onMessage}) ->
	//   {close(), post(message), closed: Promise}
	// html is a file of the script's assets; page names a built-in page
	// (form, table). onMessage(message, paths) receives the page's messages.
	n.set("appOpen", func(c goja.FunctionCall) goja.Value {
		o := c.Argument(0).ToObject(vm)
		var u string
		assets := env.Assets
		if page := n.optStr(o.Get("page"), ""); page != "" {
			if _, err := fs.Stat(webui.FS, page+".html"); err != nil {
				n.throwT("engine.internalError", page)
			}
			u, assets = ui.AssetOrigin+page+".html", webui.FS
		} else {
			html := n.optStr(o.Get("html"), "")
			var name string
			var err error
			if u, name, err = pageURL(html); err != nil {
				n.throw(err)
			}
			if name != "" {
				if _, err := fs.Stat(env.Assets, name); err != nil {
					n.throwT("app.htmlNotFound", html)
				}
			}
		}
		onMessage, _ := goja.AssertFunction(o.Get("onMessage"))
		closed, resolve, _ := vm.NewPromise()
		// The open window keeps the event loop (and the process) alive.
		env.AddPending()
		w, err := env.UI.OpenWindow(ui.WindowOptions{
			Title:  n.optStr(o.Get("title"), env.Title),
			Width:  int(o.Get("width").ToInteger()),
			Height: int(o.Get("height").ToInteger()),
			URL:    u,
			Assets: assets,
			Debug:  env.Debug,
			OnMessage: func(msg string, files []string) {
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
		obj.Set("post", func(c goja.FunctionCall) goja.Value {
			w.Post(n.str(c, 0))
			return goja.Undefined()
		})
		obj.Set("closed", closed)
		return obj
	})

	// uiForm(specJSON) -> valuesJSON | null (console and mock UIs)
	n.set("uiForm", func(c goja.FunctionCall) goja.Value {
		var spec ui.FormSpec
		if err := json.Unmarshal([]byte(n.str(c, 0)), &spec); err != nil {
			n.throw(err)
		}
		vals, ok := env.UI.Form(spec)
		if !ok {
			return goja.Null()
		}
		b, err := json.Marshal(vals)
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(string(b))
	})
	// uiTable(title, text, rows, selectable) -> [row numbers] | null
	n.set("uiTable", func(c goja.FunctionCall) goja.Value {
		sel, ok := env.UI.Table(ui.TableSpec{
			Title:      n.str(c, 0),
			Text:       n.str(c, 1),
			Rows:       int(c.Argument(2).ToInteger()),
			Selectable: n.str(c, 3),
		})
		if !ok {
			return goja.Null()
		}
		items := make([]any, len(sel))
		for i, v := range sel {
			items[i] = v
		}
		return vm.NewArray(items...)
	})
	n.set("interrupted", func(goja.FunctionCall) goja.Value { return vm.ToValue(ui.Interrupted()) })
	n.set("configDir", func(goja.FunctionCall) goja.Value {
		d, err := os.UserConfigDir()
		if err != nil {
			return goja.Null()
		}
		return vm.ToValue(filepath.Join(d, "xljs"))
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
