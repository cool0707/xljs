package ui

import (
	"bytes"
	"runtime"
	"strings"
	"sync"
	"testing"
)

func goid() string {
	buf := make([]byte, 64)
	buf = buf[:runtime.Stack(buf, false)]
	return strings.Fields(string(buf))[1]
}

// recordUI notes the goroutine every call runs on.
type recordUI struct {
	*Mock
	mu    sync.Mutex
	calls map[string]string
}

func (r *recordUI) note(name string) {
	r.mu.Lock()
	r.calls[name] = goid()
	r.mu.Unlock()
}

func (r *recordUI) Confirm(title, message string) bool {
	r.note("confirm")
	return r.Mock.Confirm(title, message)
}

func (r *recordUI) Progress(o ProgressOptions) Progress {
	r.note("progress")
	return r.Mock.Progress(o)
}

func (r *recordUI) OpenWindow(o WindowOptions) (Window, error) {
	r.note("window")
	return r.Mock.OpenWindow(o)
}

func TestRunForwardsToCaller(t *testing.T) {
	var log bytes.Buffer
	m, err := NewMockFromJSON(`{"confirm": [true], "progress": [{"cancelAfter": 2}]}`, &log)
	if err != nil {
		t.Fatal(err)
	}
	u := &recordUI{Mock: m, calls: map[string]string{}}
	caller := goid()
	var script string
	closed := make(chan struct{})
	Run(u, func(su UI) {
		script = goid()
		if su.Name() != "mock" || !su.Confirm("t", "m") {
			t.Error("confirm was not forwarded")
		}
		p := su.Progress(ProgressOptions{Title: "p", Total: 3})
		p.Update(1, 3, "")
		if p.Cancelled() {
			t.Error("cancelled too early")
		}
		p.Update(2, 3, "")
		if !p.Cancelled() {
			t.Error("updates must be applied before Cancelled is answered")
		}
		p.Close()
		w, err := su.OpenWindow(WindowOptions{Title: "w", OnClosed: func() { close(closed) }})
		if err != nil {
			t.Fatal(err)
		}
		w.Post("hello") // after close: ignored
		w.Close()
	})
	<-closed
	if script == caller {
		t.Fatal("the script ran on the calling goroutine")
	}
	for name, id := range u.calls {
		if id != caller {
			t.Errorf("%s ran on goroutine %s, want the caller (%s)", name, id, caller)
		}
	}
	if len(u.calls) != 3 {
		t.Errorf("calls = %v", u.calls)
	}
	if !strings.Contains(log.String(), "progress closed at 2/3") {
		t.Errorf("log:\n%s", log.String())
	}
}
