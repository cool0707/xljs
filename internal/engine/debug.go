package engine

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io/fs"
	"path"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/cool0707/xljs/internal/api"
	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/dop251/goja"
	"github.com/go-sourcemap/sourcemap"
)

// DebugPage is the URL of the debugger window (internal/ui/web/debug.html).
const DebugPage = ui.AssetOrigin + ui.BuiltinPrefix + "debug.html"

// Messages of the debugger window: its close button, and its calls
// (debugCall + request) and their answers (debugAnswer + response). Events
// are JSON objects (they start with "{").
const (
	debugClose  = "close"
	debugCall   = "call:"
	debugAnswer = "R"
)

// debugSession runs a script in the debugger window (xljs --debug): the
// prelude and the script run in the page (its worker), where the developer tools of
// WebView2 debug them, and call the natives here through api.Remote. This
// side keeps the event loop, so that windows, progress and the end of the
// run behave as they do with goja.
type debugSession struct {
	l         *loop
	env       *api.Env
	remote    *api.Remote
	w         ui.Window
	sourceMap *sourcemap.Consumer // of the bundle

	// The page's last idle report: the number of events it had handled
	// (-1 = it has called in since), whether the script's function has
	// settled and how the script failed.
	idleAt  int
	settled bool
	failure *string

	trace   func(string)
	started bool
	done    bool
	code    int
	err     error
}

type debugRequest struct {
	Op      string            `json:"op"`
	Name    string            `json:"name"`
	Ref     int64             `json:"ref"`
	Args    []json.RawMessage `json:"args"`
	Code    int               `json:"code"`
	Seen    int               `json:"seen"`
	Settled bool              `json:"settled"`
	Error   *string           `json:"error"`
}

func runDebug(o Options, code []byte) (int, error) {
	vm := goja.New()
	l := newLoop(vm)
	script := absPath(o.ScriptPath)
	if o.Assets == nil {
		o.Assets = o.Meta.AssetFS(filepath.Dir(script))
	}
	s := &debugSession{l: l, idleAt: -1, sourceMap: inlineSourceMap(code)}
	env := &api.Env{
		UI:         o.UI,
		Args:       o.Args,
		Script:     script,
		Version:    o.Version,
		Stdout:     o.Stdout,
		Stderr:     o.Stderr,
		Exit:       func(code int) { s.finish(code, nil) },
		Title:      o.Meta.Title,
		Meta:       o.Meta,
		Assets:     o.Assets,
		Debug:      true,
		AddPending: l.addPending,
		Post:       l.post,
	}
	defer env.Cleanup()
	s.env = env
	s.remote = api.NewRemote(vm, env)
	l.after = s.check

	trace := func(string) {}
	if o.debugTrace != nil {
		trace = o.debugTrace
	}

	// The debugger window keeps the loop running until it is closed.
	l.addPending()
	page := DebugPage
	if o.debugAutoRun {
		page += "?autorun"
	}
	w, err := o.UI.OpenWindow(ui.WindowOptions{
		Title:    i18n.T("debug.windowTitle", env.Title),
		Width:    600,
		Height:   300,
		URL:      page,
		Assets:   debugAssets(code),
		Debug:    true,
		DevTools: !o.debugAutoRun,
		Isolated: true,
		OnMessage: func(msg string, _ []string) {
			if req, ok := strings.CutPrefix(msg, debugCall); ok {
				// A call of the page, which waits for the answer.
				l.addPending()
				l.post(func() error {
					res := s.handle([]byte(req))
					s.w.Post(debugAnswer + string(res))
					trace("answer " + short(string(res)))
					return nil
				})
				return
			}
			if msg == debugClose {
				l.addPending()
				l.post(func() error {
					s.w.Close()
					return nil
				})
			}
		},
		OnClosed: func() {
			l.post(func() error {
				s.closed()
				return nil
			})
		},
	})
	if err != nil {
		return 1, err
	}
	s.w = w
	s.remote.Send = func(ev string) {
		trace("event " + short(ev))
		w.Post(ev)
	}
	s.trace = trace
	if err := l.run(); err != nil {
		return 1, err
	}
	return s.code, s.err
}

// handle answers a request of the page (debug.js).
func (s *debugSession) handle(body []byte) []byte {
	var r debugRequest
	s.trace("request " + short(string(body)))
	if err := json.Unmarshal(body, &r); err != nil {
		return errorResponse(err.Error())
	}
	if s.done {
		// The page stops when it gets the "done" event; until then it may
		// still call in.
		return []byte(`{"stop":true}`)
	}
	switch r.Op {
	case "init":
		// A reloaded page would run the script again on the same natives.
		if s.started {
			return errorResponse(i18n.T("debug.reloaded"))
		}
		s.started = true
		return append(append([]byte(`{"value":`), s.remote.Manifest()...), '}')
	case "call":
		s.idleAt = -1
		return s.remote.Call(r.Name, r.Args)
	case "invoke":
		s.idleAt = -1
		return s.remote.Invoke(r.Ref, r.Args)
	case "idle":
		s.idleAt, s.settled, s.failure = r.Seen, r.Settled, r.Error
	case "exit":
		s.finish(r.Code, nil)
	case "fail":
		msg := ""
		if r.Error != nil {
			msg = *r.Error
		}
		s.finish(1, &ScriptError{Message: s.pageError(msg)})
	default:
		return errorResponse("xljs: unknown request " + r.Op)
	}
	return []byte(`{"value":{"$u":0}}`)
}

// short cuts s for the trace.
func short(s string) string {
	if r := []rune(s); len(r) > 100 {
		return string(r[:100]) + "…"
	}
	return s
}

func errorResponse(msg string) []byte {
	b, _ := json.Marshal(map[string]any{"error": map[string]any{"$err": map[string]string{"name": "Error", "message": msg}}})
	return b
}

// check ends the run when the page is idle (it has handled every event)
// and nothing but the debugger window is pending.
func (s *debugSession) check() error {
	if s.done || s.idleAt < 0 || s.idleAt != s.remote.Sent() || !s.l.idle(1) {
		return nil
	}
	switch {
	case s.failure != nil:
		s.finish(1, &ScriptError{Message: s.pageError(*s.failure)})
	case !s.settled:
		s.finish(1, &ScriptError{Message: i18n.T("engine.notFinished")})
	default:
		s.finish(0, nil)
	}
	return nil
}

// finish ends the run: what the script left open is closed and the page
// shows the result. The debugger window stays open until the user closes
// it.
func (s *debugSession) finish(code int, err error) {
	if s.done {
		return
	}
	s.done, s.code, s.err = true, code, err
	s.trace(fmt.Sprintf("finish %d %v", code, err))
	s.env.Cleanup()
	msg := ""
	if err != nil {
		msg = err.Error()
	}
	b, _ := json.Marshal(map[string]any{"ev": "done", "code": code, "error": msg})
	s.w.Post(string(b))
}

// closed is called when the debugger window has closed. A script still
// running is stopped.
func (s *debugSession) closed() {
	if s.done {
		return
	}
	s.done, s.code = true, 130
	s.trace("closed while running")
	fmt.Fprintln(s.env.Stderr, i18n.T("debug.aborted"))
	s.env.Cleanup()
}

const (
	preludePrefix = "globalThis.__xljsPrelude = "
	bundlePrefix  = "globalThis.__xljsMain = "
)

var (
	bundleFrame = regexp.MustCompile(`[^\s(]*` + regexp.QuoteMeta(ui.BuiltinPrefix+"debug/bundle.js") + `:(\d+):(\d+)`)
	// The worker evaluates the bundle: its top level shows as "eval".
	evalFrame = regexp.MustCompile(`at eval \(([^()\s]+)\)`)
	pageFrame = regexp.MustCompile(`(?m)^\s*at .*` + regexp.QuoteMeta(ui.BuiltinPrefix) + `(?:debug\.js|debug-page\.js|debug-worker\.js|debug/prelude\.js).*$\n?`)
)

// pageError makes an error of the page look like one from goja: frames of
// the bundle point to the script's files (with the bundle's source map)
// and the page's own frames are left out.
func (s *debugSession) pageError(msg string) string {
	msg = bundleFrame.ReplaceAllStringFunc(msg, func(frame string) string {
		m := bundleFrame.FindStringSubmatch(frame)
		line, _ := strconv.Atoi(m[1])
		col, _ := strconv.Atoi(m[2])
		if line == 1 {
			col -= len(bundlePrefix)
		}
		if s.sourceMap != nil {
			if src, _, l, c, ok := s.sourceMap.Source(line, col-1); ok {
				return fmt.Sprintf("%s:%d:%d", src, l, c+1)
			}
		}
		return frame
	})
	msg = evalFrame.ReplaceAllString(msg, "at $1")
	return i18n.Text(cleanStack(pageFrame.ReplaceAllString(msg, "")))
}

// inlineSourceMap returns the source map at the end of a bundle.
func inlineSourceMap(code []byte) *sourcemap.Consumer {
	const marker = "//# sourceMappingURL=data:application/json;base64,"
	i := bytes.LastIndex(code, []byte(marker))
	if i < 0 {
		return nil
	}
	data, err := base64.StdEncoding.DecodeString(strings.TrimSpace(string(code[i+len(marker):])))
	if err != nil {
		return nil
	}
	sm, err := sourcemap.Parse("", data)
	if err != nil {
		return nil
	}
	return sm
}

// debugAssets serves the debugger window: the built-in pages, the prelude
// and the bundled script (as functions that the page runs).
func debugAssets(code []byte) fs.FS {
	return debugFS{
		ui.BuiltinPrefix + "debug/prelude.js": []byte(preludePrefix + api.PreludeSource()),
		ui.BuiltinPrefix + "debug/bundle.js":  append([]byte(bundlePrefix), code...),
	}
}

// debugFS is a few files in memory over ui.BuiltinFS (under ui.BuiltinPrefix).
type debugFS map[string][]byte

func (d debugFS) Open(name string) (fs.File, error) {
	if data, ok := d[name]; ok {
		return &memFile{Reader: bytes.NewReader(data), name: name, size: int64(len(data))}, nil
	}
	if rest, ok := strings.CutPrefix(name, ui.BuiltinPrefix); ok {
		return ui.BuiltinFS.Open(rest)
	}
	return nil, &fs.PathError{Op: "open", Path: name, Err: fs.ErrNotExist}
}

type memFile struct {
	*bytes.Reader
	name string
	size int64
}

func (f *memFile) Stat() (fs.FileInfo, error) { return f, nil }
func (f *memFile) Close() error               { return nil }
func (f *memFile) Name() string               { return path.Base(f.name) }
func (f *memFile) Size() int64                { return f.size }
func (f *memFile) Mode() fs.FileMode          { return 0o444 }
func (f *memFile) ModTime() time.Time         { return time.Time{} }
func (f *memFile) IsDir() bool                { return false }
func (f *memFile) Sys() any                   { return nil }
