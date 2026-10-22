package ui

import (
	"runtime"
	"sync"
	"time"
)

// uiThread runs functions on the UI thread.
type uiThread interface {
	// post queues fn to run on the UI thread. Safe from any goroutine.
	post(fn func())
	// serve runs queued functions (and, for the native UI, the window
	// message loop) until done is closed and the queue is empty.
	serve(done <-chan struct{})
	// cancelPoll is how long a progress cancellation state may be reused
	// before asking the UI thread again (0 = always ask).
	cancelPoll() time.Duration
}

// Run runs script on a new goroutine locked to its own OS thread while the
// calling goroutine serves UI requests. For the native UI the caller must
// be the thread that created it (the main thread). The UI handed to script
// forwards every call to the calling thread, so dialogs and windows keep
// responding while the script computes.
func Run(u UI, script func(UI)) {
	t := newThread(u)
	done := make(chan struct{})
	go func() {
		runtime.LockOSThread()
		defer func() {
			close(done)
			t.post(func() {}) // wake serve
		}()
		script(&threaded{u: u, t: t})
	}()
	t.serve(done)
}

// queue is a mutex-protected list of pending functions.
type queue struct {
	mu  sync.Mutex
	fns []func()
}

func (q *queue) push(fn func()) {
	q.mu.Lock()
	q.fns = append(q.fns, fn)
	q.mu.Unlock()
}

// drain runs the queued functions, including ones queued meanwhile.
func (q *queue) drain() {
	for {
		q.mu.Lock()
		fns := q.fns
		q.fns = nil
		q.mu.Unlock()
		if len(fns) == 0 {
			return
		}
		for _, fn := range fns {
			fn()
		}
	}
}

// chanThread is the UI thread for UIs without a message loop (console,
// mock): a goroutine that runs queued functions.
type chanThread struct {
	q    queue
	wake chan struct{}
}

func newChanThread() *chanThread { return &chanThread{wake: make(chan struct{}, 1)} }

func (t *chanThread) post(fn func()) {
	t.q.push(fn)
	select {
	case t.wake <- struct{}{}:
	default:
	}
}

func (t *chanThread) serve(done <-chan struct{}) {
	for {
		select {
		case <-t.wake:
			t.q.drain()
		case <-done:
			t.q.drain()
			return
		}
	}
}

func (t *chanThread) cancelPoll() time.Duration { return 0 }

// call runs fn on the UI thread and waits for it.
func call(t uiThread, fn func()) {
	ch := make(chan struct{})
	t.post(func() {
		defer close(ch)
		fn()
	})
	<-ch
}

// threaded forwards UI calls to the UI thread.
type threaded struct {
	u UI
	t uiThread
}

func (p *threaded) Name() string { return p.u.Name() }

func (p *threaded) Alert(title, message string) {
	call(p.t, func() { p.u.Alert(title, message) })
}

func (p *threaded) Confirm(title, message string) (ok bool) {
	call(p.t, func() { ok = p.u.Confirm(title, message) })
	return
}

func (p *threaded) Prompt(title, message, def string) (s string, ok bool) {
	call(p.t, func() { s, ok = p.u.Prompt(title, message, def) })
	return
}

func (p *threaded) OpenFiles(o PickerOptions) (paths []string) {
	call(p.t, func() { paths = p.u.OpenFiles(o) })
	return
}

func (p *threaded) SaveFile(o PickerOptions) (s string, ok bool) {
	call(p.t, func() { s, ok = p.u.SaveFile(o) })
	return
}

func (p *threaded) PickDirectory(o PickerOptions) (s string, ok bool) {
	call(p.t, func() { s, ok = p.u.PickDirectory(o) })
	return
}

func (p *threaded) ShowError(title, message string) {
	call(p.t, func() { p.u.ShowError(title, message) })
}

func (p *threaded) ShowInFolder(paths []string) (err error) {
	call(p.t, func() { err = p.u.ShowInFolder(paths) })
	return
}

func (p *threaded) OpenPath(target string) (err error) {
	call(p.t, func() { err = p.u.OpenPath(target) })
	return
}

func (p *threaded) Progress(o ProgressOptions) Progress {
	var inner Progress
	call(p.t, func() { inner = p.u.Progress(o) })
	return &threadedProgress{p: inner, t: p.t}
}

func (p *threaded) Form(f FormSpec) (v map[string]any, ok bool) {
	call(p.t, func() { v, ok = p.u.Form(f) })
	return
}

func (p *threaded) Table(t TableSpec) (sel []int, ok bool) {
	call(p.t, func() { sel, ok = p.u.Table(t) })
	return
}

func (p *threaded) OpenWindow(o WindowOptions) (Window, error) {
	var w Window
	var err error
	call(p.t, func() { w, err = p.u.OpenWindow(o) })
	if err != nil {
		return nil, err
	}
	return &threadedWindow{w: w, t: p.t}, nil
}

// threadedProgress forwards progress calls. Updates do not wait; the
// cancellation state is polled at most every cancelPoll.
type threadedProgress struct {
	p         Progress
	t         uiThread
	checked   time.Time
	cancelled bool
}

func (p *threadedProgress) Update(done, total int, message string) {
	p.t.post(func() { p.p.Update(done, total, message) })
}

func (p *threadedProgress) Cancelled() bool {
	if p.cancelled {
		return true
	}
	if d := p.t.cancelPoll(); d > 0 && time.Since(p.checked) < d {
		return false
	}
	call(p.t, func() { p.cancelled = p.p.Cancelled() })
	p.checked = time.Now()
	return p.cancelled
}

func (p *threadedProgress) Close() { call(p.t, p.p.Close) }

// threadedWindow forwards window calls. Messages do not wait.
type threadedWindow struct {
	w Window
	t uiThread
}

func (w *threadedWindow) Post(msg string) { w.t.post(func() { w.w.Post(msg) }) }
func (w *threadedWindow) Close()          { call(w.t, w.w.Close) }
