package api

import (
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
