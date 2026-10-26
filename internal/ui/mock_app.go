package ui

import (
	"bytes"
	"encoding/json"
	"fmt"
	"reflect"
	"slices"
	"strings"
	"sync"
	"time"
)

// AppStep is one action of a mock window scenario (the "app" answers): the
// mock plays the page and talks to the script with the messages of the
// page runtime (web/xljs.js). Exactly one of the action fields is set.
//
//	{"invoke": "aggregate", "args": ["2026-04"], "expect": {"ok": true}}
//	{"emit": "progress", "args": [50]}
//	{"state": {"month": "2026-04"}}
//	{"event": "click", "selector": "#run", "detail": {"value": "x"}}
//	{"handle": "render", "result": 3}   // answer to win.invoke("render")
//	{"wait": 100}                       // milliseconds
//	{"close": true}
//
// The window closes after the last step. Arguments are in the wire format
// ({"$file": path}, {"$date": iso}, ...).
type AppStep struct {
	Invoke   string            `json:"invoke"`
	Args     []json.RawMessage `json:"args"`
	Expect   json.RawMessage   `json:"expect"`
	Emit     string            `json:"emit"`
	State    map[string]any    `json:"state"`
	Event    string            `json:"event"`
	Selector string            `json:"selector"`
	Detail   map[string]any    `json:"detail"`
	Handle   string            `json:"handle"`
	Result   json.RawMessage   `json:"result"`
	Wait     int               `json:"wait"`
	Close    bool              `json:"close"`
}

// appTimeout bounds how long a step waits for the script.
var appTimeout = 10 * time.Second

// wireMsg is a message between the script and a page.
type wireMsg struct {
	Kind     string            `json:"xljs"`
	ID       int               `json:"id,omitempty"`
	Name     string            `json:"name,omitempty"`
	Args     []json.RawMessage `json:"args,omitempty"`
	Value    json.RawMessage   `json:"value,omitempty"`
	Error    json.RawMessage   `json:"error,omitempty"`
	Key      string            `json:"key,omitempty"`
	Deleted  bool              `json:"deleted,omitempty"`
	Sel      string            `json:"sel,omitempty"`
	Op       string            `json:"op,omitempty"`
	Type     string            `json:"type,omitempty"`
	HID      int               `json:"hid,omitempty"`
	Code     string            `json:"code,omitempty"`
	Fn       bool              `json:"fn,omitempty"`
	State    json.RawMessage   `json:"state,omitempty"`
	Handlers []domHandler      `json:"handlers,omitempty"`
	Event    map[string]any    `json:"event,omitempty"`
}

type domHandler struct {
	HID  int    `json:"hid"`
	Sel  string `json:"sel"`
	Type string `json:"type"`
}

// appWindow is a mock window that plays a scenario.
type appWindow struct {
	m     *Mock
	o     WindowOptions
	steps []AppStep

	mu      sync.Mutex
	closed  bool
	nextID  int
	waiting map[int]chan wireMsg // invoke id -> reply
	dom     map[int]domHandler
	replies map[string]json.RawMessage // page functions (handle steps)
	inited  chan struct{}
	done    chan struct{}
}

func newAppWindow(m *Mock, o WindowOptions, steps []AppStep) *appWindow {
	w := &appWindow{m: m, o: o, steps: steps, waiting: map[int]chan wireMsg{}, dom: map[int]domHandler{},
		replies: map[string]json.RawMessage{}, inited: make(chan struct{}), done: make(chan struct{})}
	go w.play()
	return w
}

func (w *appWindow) send(m wireMsg) {
	b, _ := json.Marshal(m)
	w.o.OnMessage(string(b), nil)
}

func raw(v json.RawMessage) string {
	if len(v) == 0 {
		return "undefined"
	}
	var b bytes.Buffer
	if json.Compact(&b, v) != nil {
		return string(v)
	}
	return b.String()
}

func rawList(vs []json.RawMessage) string {
	s := make([]string, len(vs))
	for i, v := range vs {
		s[i] = raw(v)
	}
	return strings.Join(s, ", ")
}

func (w *appWindow) play() {
	if w.o.OnMessage == nil {
		w.Close()
		return
	}
	w.send(wireMsg{Kind: "ready"})
	select {
	case <-w.inited:
	case <-w.done:
		return
	case <-time.After(appTimeout):
		w.m.logf("app: no answer to ready")
	}
	for _, s := range w.steps {
		select {
		case <-w.done:
			return
		default:
		}
		switch {
		case s.Invoke != "":
			w.invoke(s)
		case s.Emit != "":
			w.m.logf("app: emit(%s, %s)", q(s.Emit), rawList(s.Args))
			w.send(wireMsg{Kind: "event", Name: s.Emit, Args: s.Args})
		case s.State != nil:
			keys := make([]string, 0, len(s.State))
			for k := range s.State {
				keys = append(keys, k)
			}
			slices.Sort(keys)
			for _, k := range keys {
				b, _ := json.Marshal(s.State[k])
				w.m.logf("app: state.%s = %s", k, b)
				w.send(wireMsg{Kind: "state", Key: k, Value: b})
			}
		case s.Event != "":
			w.fire(s)
		case s.Handle != "":
			w.mu.Lock()
			w.replies[s.Handle] = s.Result
			w.mu.Unlock()
		case s.Wait > 0:
			time.Sleep(time.Duration(s.Wait) * time.Millisecond)
		case s.Close:
			w.Close()
			return
		}
	}
	w.Close()
}

func (w *appWindow) invoke(s AppStep) {
	w.mu.Lock()
	w.nextID++
	id := w.nextID
	ch := make(chan wireMsg, 1)
	w.waiting[id] = ch
	w.mu.Unlock()
	w.send(wireMsg{Kind: "invoke", ID: id, Name: s.Invoke, Args: s.Args})
	call := fmt.Sprintf("app: invoke %s(%s)", s.Invoke, rawList(s.Args))
	select {
	case r := <-ch:
		if r.Kind == "error" {
			var e struct{ Name, Message string }
			json.Unmarshal(r.Error, &e)
			w.m.logf("%s -> error: %s: %s", call, e.Name, e.Message)
			return
		}
		w.m.logf("%s -> %s", call, raw(r.Value))
		if len(s.Expect) > 0 && !sameJSON(s.Expect, r.Value) {
			w.m.logf("app: invoke %s: expected %s", s.Invoke, s.Expect)
		}
	case <-w.done:
	case <-time.After(appTimeout):
		w.m.logf("%s -> timeout", call)
	}
}

// fire sends a DOM event to the handlers registered (win.$(sel).on) for the
// step's selector and type, waiting a little for late registrations.
func (w *appWindow) fire(s AppStep) {
	deadline := time.Now().Add(appTimeout)
	for {
		var hids []int
		w.mu.Lock()
		for _, h := range w.dom {
			if h.Sel == s.Selector && h.Type == s.Event {
				hids = append(hids, h.HID)
			}
		}
		w.mu.Unlock()
		if len(hids) > 0 {
			slices.Sort(hids)
			w.m.logf("app: %s %s", s.Event, s.Selector)
			for _, hid := range hids {
				ev := map[string]any{"type": s.Event, "id": "", "name": "", "dataset": map[string]any{}}
				for k, v := range s.Detail {
					ev[k] = v
				}
				w.send(wireMsg{Kind: "dom-event", HID: hid, Event: ev})
			}
			return
		}
		if time.Now().After(deadline) {
			w.m.logf("app: no %s handler for %s", s.Event, s.Selector)
			return
		}
		select {
		case <-w.done:
			return
		case <-time.After(10 * time.Millisecond):
		}
	}
}

// Post receives a message from the script: it logs it and answers the
// requests (reads, evaluate and invoke get null unless a handle step set a
// result).
func (w *appWindow) Post(msg string) {
	var m wireMsg
	if err := json.Unmarshal([]byte(msg), &m); err != nil {
		w.m.logf("window.post(%s)", q(msg))
		return
	}
	w.mu.Lock()
	if w.closed {
		w.mu.Unlock()
		return
	}
	switch m.Kind {
	case "init":
		for _, h := range m.Handlers {
			w.dom[h.HID] = h
		}
		select {
		case <-w.inited:
		default:
			close(w.inited)
		}
	case "result", "error":
		if ch := w.waiting[m.ID]; ch != nil {
			delete(w.waiting, m.ID)
			ch <- m
		}
	case "dom":
		switch m.Op {
		case "on":
			w.dom[m.HID] = domHandler{HID: m.HID, Sel: m.Sel, Type: m.Type}
		case "off":
			delete(w.dom, m.HID)
		}
	}
	reply, hasReply := w.replies[m.Name]
	w.mu.Unlock()

	switch m.Kind {
	case "init":
		w.m.logf("window.init(%s)", raw(m.State))
	case "state":
		if m.Deleted {
			w.m.logf("window.state.%s deleted", m.Key)
		} else {
			w.m.logf("window.state.%s = %s", m.Key, raw(m.Value))
		}
	case "event":
		w.m.logf("window.emit(%s, %s)", q(m.Name), rawList(m.Args))
	case "dom":
		switch {
		case m.Op == "on" || m.Op == "off":
			w.m.logf("window.$(%s).%s(%s)", q(m.Sel), m.Op, q(m.Type))
		case m.ID != 0:
			w.m.logf("window.$(%s).%s(%s) -> null", q(m.Sel), m.Op, rawList(m.Args))
			w.send(wireMsg{Kind: "result", ID: m.ID, Value: json.RawMessage("null")})
		default:
			w.m.logf("window.$(%s).%s(%s)", q(m.Sel), m.Op, rawList(m.Args))
		}
	case "invoke":
		if !hasReply || len(reply) == 0 {
			reply = json.RawMessage("null")
		}
		w.m.logf("window.invoke(%s, %s) -> %s", q(m.Name), rawList(m.Args), reply)
		w.send(wireMsg{Kind: "result", ID: m.ID, Value: reply})
	case "eval":
		w.m.logf("window.evaluate(%s) -> null", q(firstLine(m.Code)))
		w.send(wireMsg{Kind: "result", ID: m.ID, Value: json.RawMessage("null")})
	}
}

func (w *appWindow) Close() {
	w.mu.Lock()
	if w.closed {
		w.mu.Unlock()
		return
	}
	w.closed = true
	close(w.done)
	w.mu.Unlock()
	w.m.logf("window.closed(%s)", q(w.o.Title))
	if w.o.OnClosed != nil {
		w.o.OnClosed()
	}
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return s[:i] + " ..."
	}
	return s
}

func sameJSON(a, b json.RawMessage) bool {
	var x, y any
	if json.Unmarshal(a, &x) != nil || json.Unmarshal(bytes.TrimSpace(b), &y) != nil {
		return bytes.Equal(a, b)
	}
	return reflect.DeepEqual(x, y)
}
