package api

import (
	"encoding/json"
	"os"
	"path/filepath"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/dop251/goja"
)

func (n *natives) pickerOptions(v goja.Value) ui.PickerOptions {
	var o ui.PickerOptions
	if isNullish(v) {
		return o
	}
	obj := v.ToObject(n.vm)
	o.Title = n.optStr(obj.Get("title"), "")
	o.Multiple = obj.Get("multiple") != nil && obj.Get("multiple").ToBoolean()
	o.StartIn = n.optStr(obj.Get("startIn"), "")
	o.SuggestedName = n.optStr(obj.Get("suggestedName"), "")
	for _, fv := range n.elems(obj.Get("filters")) {
		fo := fv.ToObject(n.vm)
		o.Filters = append(o.Filters, ui.FileFilter{
			Description: n.optStr(fo.Get("description"), ""),
			Patterns:    n.strings(fo.Get("patterns")),
		})
	}
	return o
}

func (n *natives) installUI() {
	vm, u := n.vm, n.env.UI
	n.set("uiAlert", func(c goja.FunctionCall) goja.Value {
		u.Alert(n.str(c, 0), n.str(c, 1))
		return goja.Undefined()
	})
	n.set("uiConfirm", func(c goja.FunctionCall) goja.Value {
		return vm.ToValue(u.Confirm(n.str(c, 0), n.str(c, 1)))
	})
	n.set("uiPrompt", func(c goja.FunctionCall) goja.Value {
		s, ok := u.Prompt(n.str(c, 0), n.str(c, 1), n.str(c, 2))
		if !ok {
			return goja.Null()
		}
		return vm.ToValue(s)
	})
	n.set("uiOpen", func(c goja.FunctionCall) goja.Value {
		return n.strArray(u.OpenFiles(n.pickerOptions(c.Argument(0))))
	})
	n.set("uiSave", func(c goja.FunctionCall) goja.Value {
		p, ok := u.SaveFile(n.pickerOptions(c.Argument(0)))
		if !ok {
			return goja.Null()
		}
		return vm.ToValue(p)
	})
	n.set("uiDirectory", func(c goja.FunctionCall) goja.Value {
		p, ok := u.PickDirectory(n.pickerOptions(c.Argument(0)))
		if !ok {
			return goja.Null()
		}
		return vm.ToValue(p)
	})
	n.set("uiError", func(c goja.FunctionCall) goja.Value {
		u.ShowError(n.str(c, 0), n.str(c, 1))
		return goja.Undefined()
	})
	n.set("shellShowInFolder", func(c goja.FunctionCall) goja.Value {
		if err := u.ShowInFolder(n.strings(c.Argument(0))); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("shellOpen", func(c goja.FunctionCall) goja.Value {
		if err := u.OpenPath(n.str(c, 0)); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	// uiForm(optionsJSON) -> valuesJSON | null (UIs without windows)
	n.set("uiForm", func(c goja.FunctionCall) goja.Value {
		var o ui.FormOptions
		if err := json.Unmarshal([]byte(n.str(c, 0)), &o); err != nil {
			n.throw(err)
		}
		v, ok := u.Form(o)
		if !ok {
			return goja.Null()
		}
		b, err := json.Marshal(v)
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(string(b))
	})
	// uiTable(optionsJSON) -> selected indices | null (UIs without windows)
	n.set("uiTable", func(c goja.FunctionCall) goja.Value {
		var o ui.TableOptions
		if err := json.Unmarshal([]byte(n.str(c, 0)), &o); err != nil {
			n.throw(err)
		}
		sel, ok := u.Table(o)
		if !ok {
			return goja.Null()
		}
		items := make([]any, len(sel))
		for i, x := range sel {
			items[i] = x
		}
		return vm.NewArray(items...)
	})
	// configDir() -> folder for remembered settings (%APPDATA%\xljs), "" if unknown
	n.set("configDir", func(goja.FunctionCall) goja.Value {
		if d := os.Getenv("XLJS_CONFIG_DIR"); d != "" {
			return vm.ToValue(d)
		}
		d, err := os.UserConfigDir()
		if err != nil {
			return vm.ToValue("")
		}
		return vm.ToValue(filepath.Join(d, "xljs"))
	})
	// displayText(s) translates library messages for display (see i18n.Text).
	n.set("displayText", func(c goja.FunctionCall) goja.Value { return vm.ToValue(i18n.Text(n.str(c, 0))) })
	// uiProgress(title, total, message, cancellable) -> {update, cancelled, close}
	n.set("uiProgress", func(c goja.FunctionCall) goja.Value {
		p := u.Progress(ui.ProgressOptions{
			Title:       n.str(c, 0),
			Total:       int(c.Argument(1).ToInteger()),
			Message:     n.str(c, 2),
			Cancellable: isNullish(c.Argument(3)) || c.Argument(3).ToBoolean(),
		})
		n.env.OnCleanup(p.Close)
		obj := vm.NewObject()
		obj.Set("update", func(c goja.FunctionCall) goja.Value {
			p.Update(int(c.Argument(0).ToInteger()), int(c.Argument(1).ToInteger()), n.str(c, 2))
			return goja.Undefined()
		})
		obj.Set("cancelled", func(goja.FunctionCall) goja.Value { return vm.ToValue(p.Cancelled()) })
		obj.Set("close", func(goja.FunctionCall) goja.Value {
			p.Close()
			return goja.Undefined()
		})
		return obj
	})
}
