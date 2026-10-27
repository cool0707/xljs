package api

import (
	"path/filepath"

	"github.com/cool0707/xljs/internal/vba"
	"github.com/dop251/goja"
)

func (n *natives) installVBA() {
	// vbaRead(path) / vbaReadBytes(bytes, name): the VBA project as
	// { name, codePage, modules: [{ name, type, code, sheet }], text }, or
	// null without macros. text is vba.Text, named after the file.
	result := func(p *vba.Project, file string) goja.Value {
		if p == nil {
			return goja.Null()
		}
		mods := make([]goja.Value, len(p.Modules))
		for i, m := range p.Modules {
			var sheet any
			if m.Sheet != "" {
				sheet = m.Sheet
			}
			mods[i] = n.object(map[string]any{"name": m.Name, "type": m.Type, "code": m.Code, "sheet": sheet})
		}
		return n.object(map[string]any{
			"name":     p.Name,
			"codePage": p.CodePage,
			"modules":  n.array(mods),
			"text":     vba.Text(file, p),
		})
	}
	n.set("vbaRead", func(c goja.FunctionCall) goja.Value {
		path := n.str(c, 0)
		p, err := vba.ReadFile(path)
		if err != nil {
			n.throw(err)
		}
		return result(p, filepath.Base(path))
	})
	n.set("vbaReadBytes", func(c goja.FunctionCall) goja.Value {
		p, err := vba.Read(n.bytes(c.Argument(0)))
		if err != nil {
			n.throw(err)
		}
		return result(p, n.str(c, 1))
	})
}
