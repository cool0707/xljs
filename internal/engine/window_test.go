package engine

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/cool0707/xljs/internal/ui"
)

// pageUI pretends to be the native UI: its windows are driven by a Go
// function that plays the page side of the message protocol, so the
// script side of ui.form / ui.table runs exactly as with WebView2.
type pageUI struct {
	*ui.Mock
	t     *testing.T
	pages []func(p *fakePage)
	mu    sync.Mutex
}

func (u *pageUI) Name() string { return "native" }

func (u *pageUI) OpenWindow(o ui.WindowOptions) (ui.Window, error) {
	u.mu.Lock()
	if len(u.pages) == 0 {
		u.mu.Unlock()
		u.t.Errorf("unexpected window %q", o.URL)
		return nil, fmt.Errorf("unexpected window")
	}
	play := u.pages[0]
	u.pages = u.pages[1:]
	u.mu.Unlock()
	p := &fakePage{t: u.t, o: o, in: make(chan map[string]any, 1000)}
	go func() {
		defer func() {
			if r := recover(); r != nil {
				u.t.Errorf("page: %v", r)
				p.Close()
			}
		}()
		play(p)
	}()
	return p, nil
}

type fakePage struct {
	t      *testing.T
	o      ui.WindowOptions
	in     chan map[string]any
	seq    int
	events []map[string]any
	once   sync.Once
}

func (p *fakePage) Post(msg string) {
	var m map[string]any
	if err := json.Unmarshal([]byte(msg), &m); err != nil {
		p.t.Errorf("bad message %q", msg)
	}
	p.in <- m
}

func (p *fakePage) Close() { p.once.Do(func() { p.o.OnClosed() }) }

func (p *fakePage) send(m map[string]any, files []string) {
	b, _ := json.Marshal(m)
	p.o.OnMessage(string(b), files)
}

// next waits for the next message from the script.
func (p *fakePage) next() map[string]any {
	select {
	case m := <-p.in:
		return m
	case <-time.After(5 * time.Second):
		panic("timed out waiting for the script")
	}
}

// invoke calls a script handler and returns its result; events that
// arrive meanwhile are kept in p.events.
func (p *fakePage) invoke(files []string, name string, args ...any) any {
	p.seq++
	id := float64(p.seq)
	if args == nil {
		args = []any{}
	}
	p.send(map[string]any{"type": "invoke", "id": id, "name": name, "args": args}, files)
	for {
		m := p.next()
		if m["id"] == id {
			if m["type"] == "error" {
				panic(fmt.Sprintf("%s failed: %v", name, m["value"]))
			}
			return m["value"]
		}
		p.events = append(p.events, m)
	}
}

// waitEvent returns the first event with the name (looking at kept ones first).
func (p *fakePage) waitEvent(name string) map[string]any {
	for i, e := range p.events {
		if e["name"] == name {
			p.events = append(p.events[:i], p.events[i+1:]...)
			return e
		}
	}
	for {
		m := p.next()
		if m["type"] == "event" && m["name"] == name {
			return m
		}
		p.events = append(p.events, m)
	}
}

func runWithPages(t *testing.T, src, answers string, pages ...func(p *fakePage)) result {
	t.Helper()
	dir := t.TempDir()
	t.Chdir(dir)
	cfg := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", cfg)
	t.Setenv("APPDATA", cfg)
	os.MkdirAll("data", 0o755)
	os.WriteFile(filepath.Join("data", "a.xlsx"), nil, 0o644)
	var log strings.Builder
	m, err := ui.NewMockFromJSON(answers, &log)
	if err != nil {
		t.Fatal(err)
	}
	u := &pageUI{Mock: m, t: t, pages: pages}
	script := filepath.Join(dir, "w.js")
	os.WriteFile(script, []byte(src), 0o644)
	var out strings.Builder
	code, err := Run(Options{ScriptPath: script, UI: u, Stdout: &out, Stderr: io.Discard})
	if len(u.pages) != 0 {
		t.Errorf("%d windows were not opened", len(u.pages))
	}
	return result{code, err, out.String(), log.String()}
}

func TestFormWindow(t *testing.T) {
	src := `
const v = await ui.form({
  title: "集計",
  description: "説明",
  fields: [
    { name: "src", label: "フォルダ", type: "folder", required: true },
    { name: "files", type: "files", accept: [".xlsx"] },
    { name: "n", type: "number", default: 3 },
    { name: "tax", type: "checkbox" },
  ],
  submit: "実行",
});
console.log(JSON.stringify({ src: v.src.name, kind: v.src.kind, files: v.files.map((f) => f.name), n: v.n, tax: v.tax }));
`
	r := runWithPages(t, src, `{"showDirectoryPicker": ["data"]}`, func(p *fakePage) {
		if !strings.HasSuffix(p.o.URL, "/form.html") || p.o.Title != "集計" {
			t.Errorf("window %q %q", p.o.URL, p.o.Title)
		}
		def := p.invoke(nil, "init").(map[string]any)
		if def["submit"] != "実行" || def["description"] != "説明" || len(def["fields"].([]any)) != 4 {
			t.Errorf("init = %v", def)
		}
		vals := def["values"].(map[string]any)
		if vals["n"] != "3" || vals["tax"] != false {
			t.Errorf("values = %v", vals)
		}
		res := p.invoke(nil, "submit", map[string]any{"src": "", "files": []any{}, "n": "x", "tax": false}).(map[string]any)
		errs := res["errors"].(map[string]any)
		if errs["src"] != "入力してください" || errs["n"] != "数値を入力してください" {
			t.Errorf("errors = %v", errs)
		}
		if got := p.invoke(nil, "pick", "src"); got == nil || !strings.HasSuffix(got.(string), "data") {
			t.Errorf("pick = %v", got)
		}
		dropped := p.invoke([]string{"data/a.xlsx"}, "drop", "files", map[string]any{"$files": []any{0, 1}})
		if d, _ := dropped.([]any); len(d) != 1 || !strings.HasSuffix(filepath.ToSlash(d[0].(string)), "data/a.xlsx") {
			t.Errorf("drop = %v", dropped)
		}
		res = p.invoke(nil, "submit", map[string]any{"src": "data", "files": dropped, "n": "7", "tax": true}).(map[string]any)
		if res["ok"] != true {
			t.Errorf("submit = %v", res)
		}
	})
	if r.code != 0 || r.err != nil {
		t.Fatalf("code %d: %v\n%s", r.code, r.err, r.stderr)
	}
	if want := `{"src":"data","kind":"directory","files":["a.xlsx"],"n":7,"tax":true}`; strings.TrimSpace(r.stdout) != want {
		t.Fatalf("stdout = %s, want %s", r.stdout, want)
	}
}

func TestFormWindowRun(t *testing.T) {
	src := `
const v = await ui.form({
  fields: ["name"],
  onSubmit: async (vals, ctx) => {
    for (let i = 1; i <= 3; i++) { ctx.progress(i, 3, "item " + i); await sleep(1); }
    ctx.log("hello", vals.name);
    if (vals.name === "boom") throw new Error("失敗");
    return "data";
  },
});
console.log("result", v && v.name);
`
	r := runWithPages(t, src, "", func(p *fakePage) {
		p.invoke(nil, "init")
		p.invoke(nil, "submit", map[string]any{"name": "boom"})
		fin := p.waitEvent("finished")["args"].([]any)[0].(map[string]any)
		if fin["state"] != "failed" || !strings.Contains(fin["message"].(string), "失敗") {
			t.Errorf("finished = %v", fin)
		}
		p.events = nil
		p.invoke(nil, "submit", map[string]any{"name": "太郎"})
		if log := p.waitEvent("log")["args"].([]any)[0]; log != "hello 太郎" {
			t.Errorf("log = %v", log)
		}
		fin = p.waitEvent("finished")["args"].([]any)[0].(map[string]any)
		out, _ := fin["output"].(map[string]any)
		if fin["state"] != "done" || out == nil || out["dir"] != true || !strings.HasSuffix(out["path"].(string), "data") {
			t.Errorf("finished = %v", fin)
		}
		last := map[string]any{}
		for _, e := range p.events {
			if e["name"] == "progress" {
				last = e["args"].([]any)[0].(map[string]any)
			}
		}
		if last["done"] != 3.0 || last["total"] != 3.0 {
			t.Errorf("last progress = %v (events %v)", last, p.events)
		}
		p.send(map[string]any{"type": "event", "name": "xljs:close", "args": []any{}}, nil)
	})
	if r.code != 0 || r.err != nil || strings.TrimSpace(r.stdout) != "result 太郎" {
		t.Fatalf("code %d: %v\nstdout %s\n%s", r.code, r.err, r.stdout, r.stderr)
	}
}

func TestFormWindowCancel(t *testing.T) {
	r := runWithPages(t, `console.log(await ui.form({ fields: ["a"] }))`, "", func(p *fakePage) {
		p.invoke(nil, "init")
		p.Close()
	})
	if r.code != 0 || strings.TrimSpace(r.stdout) != "null" {
		t.Fatalf("code %d: %v %q", r.code, r.err, r.stdout)
	}
}

func TestTableWindow(t *testing.T) {
	src := `
const rows = [{ 名前: "A", 数: 1, 日: new Date(2026, 3, 1) }, { 名前: "B", 数: 2.5, 日: new Date(2026, 3, 2, 9, 30) }, { 名前: "C" }];
const w = ui.table(rows, { title: "表示" });
await w.closed;
const picked = await ui.table(rows, { title: "選択", selectable: "multiple" });
console.log(JSON.stringify(picked.map((r) => r.名前)));
`
	r := runWithPages(t, src, `{"showSaveFilePicker": ["out.csv"]}`,
		func(p *fakePage) {
			def := p.invoke(nil, "init").(map[string]any)
			if fmt.Sprint(def["columns"]) != "[名前 数 日]" || def["selectable"] != "" {
				t.Errorf("init = %v", def)
			}
			if got := fmt.Sprint(def["rows"]); got != "[[A 1 2026-04-01] [B 2.5 2026-04-02 09:30:00] [C <nil> <nil>]]" {
				t.Errorf("rows = %s", got)
			}
			if name := p.invoke(nil, "save", "csv", []any{2, 0}); name != "out.csv" {
				t.Errorf("save = %v", name)
			}
			p.Close()
		},
		func(p *fakePage) {
			if def := p.invoke(nil, "init").(map[string]any); def["selectable"] != "multiple" {
				t.Errorf("init = %v", def)
			}
			p.invoke(nil, "ok", []any{2, 0})
		})
	if r.code != 0 || r.err != nil || strings.TrimSpace(r.stdout) != `["C","A"]` {
		t.Fatalf("code %d: %v\nstdout %s\n%s", r.code, r.err, r.stdout, r.stderr)
	}
	b, _ := os.ReadFile("out.csv")
	if got := strings.TrimPrefix(string(b), "\ufeff"); got != "名前,数,日\r\nC,,\r\nA,1,2026/04/01\r\n" {
		t.Fatalf("csv = %q", got)
	}
}
