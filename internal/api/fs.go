package api

import (
	"errors"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	"github.com/bmatcuk/doublestar/v4"
	"github.com/dop251/goja"
)

func (n *natives) installFS() {
	vm := n.vm

	n.set("fsStat", func(c goja.FunctionCall) goja.Value {
		st, err := os.Stat(n.str(c, 0))
		if err != nil {
			if errors.Is(err, fs.ErrNotExist) {
				return goja.Null()
			}
			n.throw(err)
		}
		return n.object(map[string]any{
			"isFile":      st.Mode().IsRegular(),
			"isDirectory": st.IsDir(),
			"size":        st.Size(),
			"mtimeMs":     st.ModTime().UnixMilli(),
		})
	})
	n.set("fsReadBytes", func(c goja.FunctionCall) goja.Value {
		b, err := os.ReadFile(n.str(c, 0))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(vm.NewArrayBuffer(b))
	})
	n.set("fsReadText", func(c goja.FunctionCall) goja.Value {
		b, err := os.ReadFile(n.str(c, 0))
		if err != nil {
			n.throw(err)
		}
		s, err := decode(b, n.optStr(c.Argument(1), "auto"))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
	n.set("fsWriteBytes", func(c goja.FunctionCall) goja.Value {
		writeFile(n, n.str(c, 0), n.bytes(c.Argument(1)), c.Argument(2).ToBoolean())
		return goja.Undefined()
	})
	n.set("fsWriteText", func(c goja.FunctionCall) goja.Value {
		path := n.str(c, 0)
		appendMode := c.Argument(3).ToBoolean()
		enc := n.optStr(c.Argument(2), "utf8")
		if appendMode && isUTF8BOM(enc) {
			// Only write the BOM when creating the file.
			if st, err := os.Stat(path); err == nil && st.Size() > 0 {
				enc = "utf8"
			}
		}
		b, err := encode(n.str(c, 1), enc)
		if err != nil {
			n.throw(err)
		}
		writeFile(n, path, b, appendMode)
		return goja.Undefined()
	})
	n.set("fsReadDir", func(c goja.FunctionCall) goja.Value {
		entries, err := os.ReadDir(n.str(c, 0))
		if err != nil {
			n.throw(err)
		}
		out := make([]goja.Value, 0, len(entries))
		for _, e := range entries {
			out = append(out, n.object(map[string]any{"name": e.Name(), "isDirectory": e.IsDir()}))
		}
		return n.array(out)
	})
	n.set("fsWalk", func(c goja.FunctionCall) goja.Value {
		root := n.str(c, 0)
		include := n.strings(c.Argument(1))
		exclude := n.strings(c.Argument(2))
		dirs := c.Argument(3).ToBoolean()
		files := !c.Argument(4).ToBoolean() // 5th arg: skipFiles
		res, err := walk(root, include, exclude, dirs, files)
		if err != nil {
			n.throw(err)
		}
		return n.walkResult(res)
	})
	n.set("fsGlob", func(c goja.FunctionCall) goja.Value {
		root := n.str(c, 0)
		pattern := filepath.ToSlash(n.str(c, 1))
		res, err := walk(root, []string{pattern}, nil, false, true)
		if err != nil {
			n.throw(err)
		}
		return n.walkResult(res)
	})
	n.set("fsMkdir", func(c goja.FunctionCall) goja.Value {
		if err := os.MkdirAll(n.str(c, 0), 0o755); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("fsRemove", func(c goja.FunctionCall) goja.Value {
		p := n.str(c, 0)
		var err error
		if c.Argument(1).ToBoolean() {
			err = os.RemoveAll(p)
		} else {
			err = os.Remove(p)
		}
		if err != nil && !errors.Is(err, fs.ErrNotExist) {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("fsRename", func(c goja.FunctionCall) goja.Value {
		if err := os.Rename(n.str(c, 0), n.str(c, 1)); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("fsCopy", func(c goja.FunctionCall) goja.Value {
		if err := copyFile(n.str(c, 0), n.str(c, 1)); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("fsCwd", func(c goja.FunctionCall) goja.Value {
		d, err := os.Getwd()
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(d)
	})
	n.set("fsChdir", func(c goja.FunctionCall) goja.Value {
		if err := os.Chdir(n.str(c, 0)); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("knownFolder", func(c goja.FunctionCall) goja.Value {
		p := knownFolder(n.str(c, 0))
		if p == "" {
			return goja.Null()
		}
		return vm.ToValue(p)
	})

	// path
	n.set("pathJoin", func(c goja.FunctionCall) goja.Value {
		parts := make([]string, 0, len(c.Arguments))
		for _, a := range c.Arguments {
			parts = append(parts, a.String())
		}
		return vm.ToValue(filepath.Join(parts...))
	})
	n.set("pathResolve", func(c goja.FunctionCall) goja.Value {
		p := ""
		for _, a := range c.Arguments {
			s := a.String()
			if filepath.IsAbs(s) || p == "" {
				p = s
			} else {
				p = filepath.Join(p, s)
			}
		}
		abs, err := filepath.Abs(p)
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(abs)
	})
	n.set("pathDirname", func(c goja.FunctionCall) goja.Value { return vm.ToValue(filepath.Dir(n.str(c, 0))) })
	n.set("pathBasename", func(c goja.FunctionCall) goja.Value {
		b := filepath.Base(n.str(c, 0))
		if ext := n.str(c, 1); ext != "" && strings.HasSuffix(b, ext) && b != ext {
			b = strings.TrimSuffix(b, ext)
		}
		return vm.ToValue(b)
	})
	n.set("pathExtname", func(c goja.FunctionCall) goja.Value { return vm.ToValue(filepath.Ext(n.str(c, 0))) })
	n.set("pathNormalize", func(c goja.FunctionCall) goja.Value { return vm.ToValue(filepath.Clean(n.str(c, 0))) })
	n.set("pathIsAbsolute", func(c goja.FunctionCall) goja.Value { return vm.ToValue(filepath.IsAbs(n.str(c, 0))) })
	n.set("pathRelative", func(c goja.FunctionCall) goja.Value {
		r, err := filepath.Rel(n.str(c, 0), n.str(c, 1))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(r)
	})
	n.obj.Set("pathSep", string(filepath.Separator))
}

func (n *natives) walkResult(res []map[string]any) goja.Value {
	out := make([]goja.Value, len(res))
	for i, m := range res {
		out[i] = n.object(m)
	}
	return n.array(out)
}

func writeFile(n *natives, path string, data []byte, appendMode bool) {
	flag := os.O_CREATE | os.O_WRONLY
	if appendMode {
		flag |= os.O_APPEND
	} else {
		flag |= os.O_TRUNC
	}
	f, err := os.OpenFile(path, flag, 0o644)
	if err != nil {
		n.throw(err)
	}
	if _, err := f.Write(data); err != nil {
		f.Close()
		n.throw(err)
	}
	if err := f.Close(); err != nil {
		n.throw(err)
	}
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	st, err := in.Stat()
	if err != nil {
		return err
	}
	if st.IsDir() {
		return errors.New("copy: " + src + " is a directory")
	}
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	if err := out.Close(); err != nil {
		return err
	}
	return os.Chtimes(dst, st.ModTime(), st.ModTime())
}

// matchPattern matches a slash-separated relative path against a glob.
// Patterns without a slash match the base name at any depth. Matching is
// case-insensitive (Windows semantics).
func matchPattern(pattern, rel string) bool {
	pattern = strings.ToLower(filepath.ToSlash(pattern))
	rel = strings.ToLower(rel)
	if !strings.Contains(pattern, "/") {
		base := rel
		if i := strings.LastIndex(rel, "/"); i >= 0 {
			base = rel[i+1:]
		}
		ok, _ := doublestar.Match(pattern, base)
		return ok
	}
	pattern = strings.TrimPrefix(pattern, "./")
	ok, _ := doublestar.Match(pattern, rel)
	return ok
}

func matchAny(patterns []string, rel string) bool {
	for _, p := range patterns {
		if matchPattern(p, rel) {
			return true
		}
	}
	return false
}

func walk(root string, include, exclude []string, dirs, files bool) ([]map[string]any, error) {
	abs, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	st, err := os.Stat(abs)
	if err != nil {
		return nil, err
	}
	if !st.IsDir() {
		return nil, errors.New(root + " is not a directory")
	}
	var out []map[string]any
	err = filepath.WalkDir(abs, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			if p == abs {
				return err
			}
			// Skip unreadable entries instead of aborting the walk.
			if d != nil && d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if p == abs {
			return nil
		}
		rel, _ := filepath.Rel(abs, p)
		slash := filepath.ToSlash(rel)
		if len(exclude) > 0 && matchAny(exclude, slash) {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if d.IsDir() && !dirs {
			return nil
		}
		if !d.IsDir() && !files {
			return nil
		}
		if len(include) > 0 && !matchAny(include, slash) {
			return nil
		}
		out = append(out, map[string]any{"path": p, "rel": rel, "isDirectory": d.IsDir()})
		return nil
	})
	return out, err
}
