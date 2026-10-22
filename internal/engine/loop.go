package engine

import (
	"sync"
	"time"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/dop251/goja"
)

// loop is a minimal event loop: timers plus callbacks posted from other
// goroutines. Promise jobs are run by goja itself whenever a call into the
// VM returns.
type loop struct {
	vm      *goja.Runtime
	timers  map[int64]*timer
	nextID  int64
	seq     int64
	mu      sync.Mutex
	posted  []func() error
	pending int
	wake    chan struct{}
}

type timer struct {
	id       int64
	seq      int64
	when     time.Time
	interval time.Duration
	repeat   bool
	fn       goja.Callable
	args     []goja.Value
}

func newLoop(vm *goja.Runtime) *loop {
	return &loop{vm: vm, timers: map[int64]*timer{}, wake: make(chan struct{}, 1)}
}

func (l *loop) install() {
	vm := l.vm
	add := func(repeat bool) func(goja.FunctionCall) goja.Value {
		return func(call goja.FunctionCall) goja.Value {
			fn, ok := goja.AssertFunction(call.Argument(0))
			if !ok {
				panic(vm.NewTypeError(i18n.T("engine.callbackNotFunction")))
			}
			delay := call.Argument(1).ToFloat()
			if delay != delay || delay < 0 { // NaN or negative
				delay = 0
			}
			var args []goja.Value
			if len(call.Arguments) > 2 {
				args = append(args, call.Arguments[2:]...)
			}
			l.nextID++
			l.seq++
			d := time.Duration(delay * float64(time.Millisecond))
			if repeat && d < time.Millisecond {
				d = time.Millisecond
			}
			t := &timer{id: l.nextID, seq: l.seq, when: time.Now().Add(d), interval: d, repeat: repeat, fn: fn, args: args}
			l.timers[t.id] = t
			return vm.ToValue(t.id)
		}
	}
	clear := func(call goja.FunctionCall) goja.Value {
		delete(l.timers, call.Argument(0).ToInteger())
		return goja.Undefined()
	}
	vm.Set("setTimeout", add(false))
	vm.Set("setInterval", add(true))
	vm.Set("setImmediate", func(call goja.FunctionCall) goja.Value {
		args := append([]goja.Value{call.Argument(0), vm.ToValue(0)}, call.Arguments[min(1, len(call.Arguments)):]...)
		return add(false)(goja.FunctionCall{This: call.This, Arguments: args})
	})
	vm.Set("clearTimeout", clear)
	vm.Set("clearInterval", clear)
	vm.Set("clearImmediate", clear)
}

// post schedules fn to run on the loop thread. It is safe to call from any
// goroutine; the caller must have called addPending beforehand.
func (l *loop) post(fn func() error) {
	l.mu.Lock()
	l.posted = append(l.posted, fn)
	l.pending--
	l.mu.Unlock()
	select {
	case l.wake <- struct{}{}:
	default:
	}
}

func (l *loop) addPending() {
	l.mu.Lock()
	l.pending++
	l.mu.Unlock()
}

// run processes timers and posted callbacks until nothing is left.
func (l *loop) run() error {
	for {
		l.mu.Lock()
		posted := l.posted
		l.posted = nil
		pending := l.pending
		l.mu.Unlock()
		for _, fn := range posted {
			if err := fn(); err != nil {
				return err
			}
		}
		if len(posted) > 0 {
			continue
		}

		next := l.nextTimer()
		if next == nil {
			if pending == 0 {
				return nil
			}
			<-l.wake
			continue
		}
		if wait := time.Until(next.when); wait > 0 {
			if pending > 0 {
				select {
				case <-l.wake:
					continue
				case <-time.After(wait):
				}
			} else {
				time.Sleep(wait)
			}
		}
		if next.repeat {
			l.seq++
			next.seq = l.seq
			next.when = time.Now().Add(next.interval)
		} else {
			delete(l.timers, next.id)
		}
		if _, err := next.fn(goja.Undefined(), next.args...); err != nil {
			return err
		}
	}
}

func (l *loop) nextTimer() *timer {
	var best *timer
	for _, t := range l.timers {
		if best == nil || t.when.Before(best.when) || (t.when.Equal(best.when) && t.seq < best.seq) {
			best = t
		}
	}
	return best
}
