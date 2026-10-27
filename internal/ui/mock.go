package ui

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strconv"
	"sync"
)

// Mock answers UI requests from a prepared queue and logs every call.
// It is meant for unattended runs and automated tests.
//
// Answers file format (all keys optional):
//
//	{
//	  "confirm": [true, false],
//	  "prompt": ["text", null],
//	  "showOpenFilePicker": [["a.xlsx", "b.xlsx"], []],
//	  "showSaveFilePicker": ["out.xlsx", null],
//	  "showDirectoryPicker": ["data", null],
//	  "progress": [{"cancelAfter": 3}],
//	  "form": [{"src": "data", "tax": true}, null],
//	  "table": [[0, 2], null],
//	  "app": [[{"invoke": "aggregate", "args": ["2026-04"]}, {"close": true}]]
//	}
//
// When a queue is empty the call behaves as if the user cancelled. A form
// answer lists the values that differ from the current ones; a table
// answer lists the indices of the chosen rows. Each window (OpenWindow)
// plays the next "app" scenario (see AppStep) and then closes; without one
// it is logged and closed immediately.
type Mock struct {
	Log io.Writer

	mu      sync.Mutex
	logMu   sync.Mutex // windows log from their own goroutine
	answers map[string][]json.RawMessage
}

// NewMock creates a mock UI. answersPath may be empty.
func NewMock(answersPath string, log io.Writer) (*Mock, error) {
	m := &Mock{Log: log, answers: map[string][]json.RawMessage{}}
	if answersPath == "" {
		return m, nil
	}
	data, err := os.ReadFile(answersPath)
	if err != nil {
		return nil, err
	}
	if err := json.Unmarshal(data, &m.answers); err != nil {
		return nil, fmt.Errorf("%s: %w", answersPath, err)
	}
	return m, nil
}

// NewMockFromJSON creates a mock UI from an answers JSON document.
func NewMockFromJSON(doc string, log io.Writer) (*Mock, error) {
	m := &Mock{Log: log, answers: map[string][]json.RawMessage{}}
	if doc != "" {
		if err := json.Unmarshal([]byte(doc), &m.answers); err != nil {
			return nil, err
		}
	}
	return m, nil
}

func (m *Mock) Name() string { return "mock" }

func (m *Mock) next(key string, dst any) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	q := m.answers[key]
	if len(q) == 0 {
		return false
	}
	m.answers[key] = q[1:]
	if string(q[0]) == "null" {
		return false
	}
	if err := json.Unmarshal(q[0], dst); err != nil {
		m.logf("%s: invalid answer %s: %v", key, q[0], err)
		return false
	}
	return true
}

func (m *Mock) logf(format string, args ...any) {
	if m.Log != nil {
		m.logMu.Lock()
		fmt.Fprintf(m.Log, "[mock] "+format+"\n", args...)
		m.logMu.Unlock()
	}
}

func q(s string) string { return strconv.Quote(s) }

func (m *Mock) Alert(title, message string) { m.logf("alert(%s)", q(message)) }

func (m *Mock) Confirm(title, message string) bool {
	var v bool
	m.next("confirm", &v)
	m.logf("confirm(%s) -> %v", q(message), v)
	return v
}

func (m *Mock) Prompt(title, message, def string) (string, bool) {
	var v string
	ok := m.next("prompt", &v)
	if ok {
		m.logf("prompt(%s) -> %s", q(message), q(v))
	} else {
		m.logf("prompt(%s) -> null", q(message))
	}
	return v, ok
}

func (m *Mock) OpenFiles(o PickerOptions) []string {
	var v []string
	m.next("showOpenFilePicker", &v)
	if !o.Multiple && len(v) > 1 {
		v = v[:1]
	}
	m.logf("showOpenFilePicker() -> %q", v)
	return v
}

func (m *Mock) SaveFile(o PickerOptions) (string, bool) {
	var v string
	ok := m.next("showSaveFilePicker", &v)
	m.logf("showSaveFilePicker(%s) -> %s", q(o.SuggestedName), q(v))
	return v, ok && v != ""
}

func (m *Mock) PickDirectory(o PickerOptions) (string, bool) {
	var v string
	ok := m.next("showDirectoryPicker", &v)
	m.logf("showDirectoryPicker() -> %s", q(v))
	return v, ok && v != ""
}

func (m *Mock) ShowError(title, message string) { m.logf("error: %s", message) }

func (m *Mock) Progress(o ProgressOptions) Progress {
	var cfg struct {
		CancelAfter int `json:"cancelAfter"`
	}
	m.next("progress", &cfg)
	m.logf("progress(%s, total=%d) start", q(o.Title), o.Total)
	activeProgress.Add(1)
	return &mockProgress{m: m, cancelAfter: cfg.CancelAfter, total: o.Total}
}

type mockProgress struct {
	m           *Mock
	cancelAfter int
	updates     int
	done, total int
	closed      bool
}

func (p *mockProgress) Update(done, total int, message string) {
	p.updates++
	p.done = done
	if total > 0 {
		p.total = total
	}
}

func (p *mockProgress) Cancelled() bool {
	return (p.cancelAfter > 0 && p.updates >= p.cancelAfter) || Interrupted()
}

func (p *mockProgress) Close() {
	if p.closed {
		return
	}
	p.closed = true
	activeProgress.Add(-1)
	p.m.logf("progress closed at %d/%d", p.done, p.total)
}

func (m *Mock) ShowInFolder(paths []string) error {
	m.logf("showItemInFolder(%q)", paths)
	return checkPaths(paths)
}

func (m *Mock) OpenPath(target string) error {
	m.logf("openPath(%s)", q(target))
	return nil
}

// SupportsWindows is false: forms and tables are answered from the queue.
func (m *Mock) SupportsWindows() bool { return false }

func (m *Mock) Form(o FormOptions) (map[string]any, bool) {
	if o.Error != "" {
		m.logf("form error: %s", o.Error)
	}
	for _, f := range o.Fields {
		if f.Error != "" {
			m.logf("form error: %s: %s", f.Name, f.Error)
		}
		for _, e := range f.CellErrors {
			m.logf("form error: %s: %s", f.Name, cellErrorText(f, e))
		}
	}
	v := map[string]any{}
	if !m.next("form", &v) {
		m.logf("form(%s) -> null", q(o.Title))
		return nil, false
	}
	b, _ := json.Marshal(v)
	m.logf("form(%s) -> %s", q(o.Title), b)
	return v, true
}

func (m *Mock) Table(o TableOptions) ([]int, bool) {
	m.logf("table(%s, %d rows, %q)", q(o.Title), len(o.Rows), o.Columns)
	if o.Select == SelectNone {
		return nil, true
	}
	var v []int
	if !m.next("table", &v) {
		m.logf("table -> cancel")
		return nil, false
	}
	if o.Select == SelectSingle && len(v) > 1 {
		v = v[:1]
	}
	m.logf("table -> %v", v)
	return v, true
}

// OpenWindow logs the window and closes it right away, as if the user had
// closed it without doing anything.
func (m *Mock) OpenWindow(o WindowOptions) (Window, error) {
	m.logf("window.open(%s, %s, %dx%d)", q(o.Title), q(o.URL), o.Width, o.Height)
	if len(o.Icon) > 0 {
		m.logf("window.icon(%d bytes)", len(o.Icon))
	}
	var steps []AppStep
	if m.next("app", &steps) {
		return newAppWindow(m, o, steps), nil
	}
	w := &mockWindow{m: m, o: o}
	w.Close()
	return w, nil
}

type mockWindow struct {
	m      *Mock
	o      WindowOptions
	closed bool
}

func (w *mockWindow) Post(msg string) {
	if !w.closed {
		w.m.logf("window.post(%s)", q(msg))
	}
}

func (w *mockWindow) Close() {
	if w.closed {
		return
	}
	w.closed = true
	w.m.logf("window.closed(%s)", q(w.o.Title))
	if w.o.OnClosed != nil {
		w.o.OnClosed()
	}
}
