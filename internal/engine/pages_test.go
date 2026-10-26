package engine

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/cool0707/xljs/internal/ui"
)

// pageUI is a mock UI that "shows" forms and tables as windows: each
// window opened runs the next fake page, which talks to the script with
// the same JSON messages as internal/ui/web/form.js and table.js.
type pageUI struct {
	*ui.Mock
	t     *testing.T
	mu    sync.Mutex
	pages []func(p *fakePage)
	urls  []string
	wg    sync.WaitGroup
}

func (u *pageUI) SupportsWindows() bool { return true }

func (u *pageUI) OpenWindow(o ui.WindowOptions) (ui.Window, error) {
	u.mu.Lock()
	defer u.mu.Unlock()
	u.urls = append(u.urls, o.URL)
	if len(u.pages) == 0 {
		u.t.Errorf("unexpected window %s", o.URL)
		return u.Mock.OpenWindow(o)
	}
	run := u.pages[0]
	u.pages = u.pages[1:]
	p := &fakePage{t: u.t, o: o, in: make(chan map[string]any, 10000), done: make(chan struct{})}
	u.wg.Add(1)
	go func() {
		defer u.wg.Done()
		p.send(map[string]any{"type": "ready"})
		run(p)
	}()
	return p, nil
}

type fakePage struct {
	t      *testing.T
	o      ui.WindowOptions
	in     chan map[string]any
	once   sync.Once
	done   chan struct{}
	closed bool
	mu     sync.Mutex
}

func (p *fakePage) Post(msg string) {
	var m map[string]any
	if err := json.Unmarshal([]byte(msg), &m); err != nil {
		p.t.Errorf("bad message %q: %v", msg, err)
		return
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if !p.closed {
		p.in <- m
	}
}

func (p *fakePage) Close() {
	p.once.Do(func() {
		p.mu.Lock()
		p.closed = true
		p.mu.Unlock()
		close(p.done)
		p.o.OnClosed()
	})
}

func (p *fakePage) send(m map[string]any, files ...string) {
	b, _ := json.Marshal(m)
	p.o.OnMessage(string(b), files)
}

// expect returns the next message of the given type; others are skipped
// and collected in skipped.
func (p *fakePage) expect(typ string, skipped *[]map[string]any) map[string]any {
	p.t.Helper()
	timeout := time.After(5 * time.Second)
	for {
		select {
		case m := <-p.in:
			if m["type"] == typ {
				return m
			}
			if skipped != nil {
				*skipped = append(*skipped, m)
			}
		case <-timeout:
			p.t.Errorf("timed out waiting for %q", typ)
			return map[string]any{}
		}
	}
}

func (p *fakePage) waitClosed() {
	p.t.Helper()
	select {
	case <-p.done:
	case <-time.After(5 * time.Second):
		p.t.Error("window was not closed")
	}
}

func asJSON(v any) string {
	b, _ := json.Marshal(v)
	return string(b)
}

func TestFormAndTableWindows(t *testing.T) {
	var log bytes.Buffer
	m, err := ui.NewMockFromJSON(`{"showDirectoryPicker": ["data"], "showSaveFilePicker": ["out.csv", "out.xlsx"]}`, &log)
	if err != nil {
		t.Fatal(err)
	}
	u := &pageUI{Mock: m, t: t}
	dir := t.TempDir()
	t.Chdir(dir)
	t.Setenv("XLJS_CONFIG_DIR", t.TempDir())
	os.MkdirAll(filepath.Join(dir, "data"), 0o755)
	os.WriteFile(filepath.Join(dir, "data", "a.xlsx"), nil, 0o644)
	os.WriteFile(filepath.Join(dir, "data", "b.txt"), nil, 0o644)

	u.pages = []func(p *fakePage){
		// 1. Validation, picker and drop, then a valid submit closes the window.
		func(p *fakePage) {
			init := p.expect("init", nil)
			if got := asJSON(init["fields"]); !strings.Contains(got, `"label":"フォルダ"`) || !strings.Contains(got, `"accept":[".xlsx"]`) ||
				!strings.Contains(got, `"value":"顧客別"`) || !strings.Contains(got, `"value":"2026-04-01"`) {
				t.Errorf("init fields = %s", got)
			}
			if texts := init["texts"].(map[string]any); texts["browse"] != "参照…" {
				t.Errorf("texts = %v", texts)
			}
			p.send(map[string]any{"type": "submit", "values": map[string]any{"src": "", "n": "x"}})
			if e := asJSON(p.expect("errors", nil)); !strings.Contains(e, `"src":"入力してください"`) || !strings.Contains(e, `"n":"数値を入力してください"`) {
				t.Errorf("errors = %s", e)
			}
			p.send(map[string]any{"type": "pick", "field": "src", "value": ""})
			if got := p.expect("picked", nil); got["field"] != "src" || got["value"] != filepath.Join(dir, "data") || got["append"] != false {
				t.Errorf("picked = %s", got)
			}
			p.send(map[string]any{"type": "drop", "field": "inputs"}, filepath.Join(dir, "data", "a.xlsx"), filepath.Join(dir, "data", "b.txt"))
			if got := asJSON(p.expect("picked", nil)); !strings.Contains(got, `"append":true`) || !strings.Contains(got, "a.xlsx") || strings.Contains(got, "b.txt") {
				t.Errorf("dropped = %s", got)
			}
			p.send(map[string]any{"type": "submit", "values": map[string]any{
				"src": "data", "n": "2", "mode": "商品別", "day": "2026-05-06", "inputs": []string{"data/a.xlsx"}}})
			p.waitClosed()
		},
		// 2. onSubmit: progress and log, output folder, run again, close.
		func(p *fakePage) {
			p.expect("init", nil)
			p.send(map[string]any{"type": "submit", "values": map[string]any{"n": "3"}})
			p.expect("running", nil)
			var updates []map[string]any
			done := p.expect("done", &updates)
			if got := asJSON(done); got != `{"cancelled":false,"ok":true,"output":true,"type":"done"}` {
				t.Errorf("done = %s", got)
			}
			all := asJSON(updates)
			if !strings.Contains(all, `"item 1"`) || !strings.Contains(all, `"item 3"`) {
				t.Errorf("log = %s", all)
			}
			if !strings.Contains(all, `"done":3,"message":"","total":3,"type":"progress"`) {
				t.Errorf("progress = %s", all)
			}
			p.send(map[string]any{"type": "open"})
			p.send(map[string]any{"type": "close"})
			p.waitClosed()
		},
		// 3. Cancelling a run.
		func(p *fakePage) {
			p.expect("init", nil)
			p.send(map[string]any{"type": "submit", "values": map[string]any{}})
			p.expect("running", nil)
			p.send(map[string]any{"type": "cancel"})
			if got := asJSON(p.expect("done", nil)); got != `{"cancelled":true,"ok":true,"output":false,"type":"done"}` {
				t.Errorf("done = %s", got)
			}
			p.send(map[string]any{"type": "close"})
			p.waitClosed()
		},
		// 4. An error in onSubmit is shown in the window.
		func(p *fakePage) {
			p.expect("init", nil)
			p.send(map[string]any{"type": "submit", "values": map[string]any{}})
			done := p.expect("done", nil)
			if done["ok"] != false || !strings.Contains(done["error"].(string), "Error: 失敗") || !strings.Contains(done["error"].(string), "w.js:") {
				t.Errorf("done = %v", done)
			}
			p.send(map[string]any{"type": "close"})
			p.waitClosed()
		},
		// 5. Table: rows, save as CSV, select.
		func(p *fakePage) {
			init := p.expect("init", nil)
			if got := asJSON(init["columns"]); got != `[{"label":"名前","width":0},{"label":"数","width":0},{"label":"日","width":0}]` || init["total"] != 3.0 || init["select"] != "multiple" {
				t.Errorf("init = %v", init)
			}
			if got := asJSON(p.expect("rows", nil)["rows"]); got != `[["a",1,"2026-04-01"],["b",2,null],["c",null,"2026-04-01 12:30:00"]]` {
				t.Errorf("rows = %s", got)
			}
			p.send(map[string]any{"type": "save", "format": "csv", "indices": []int{2, 0}})
			if got := p.expect("saved", nil); !strings.HasSuffix(got["path"].(string), "out.csv") {
				t.Errorf("saved = %v", got)
			}
			p.send(map[string]any{"type": "save", "format": "xlsx", "indices": nil})
			p.expect("saved", nil)
			p.send(map[string]any{"type": "ok", "indices": []int{0, 2, 7}})
			p.waitClosed()
		},
		// 6. Display-only table with a double-click handler that closes it.
		func(p *fakePage) {
			if init := p.expect("init", nil); init["dblclick"] != true {
				t.Errorf("init = %v", init)
			}
			p.expect("rows", nil)
			p.send(map[string]any{"type": "dblclick", "index": 1, "column": 1})
			p.send(map[string]any{"type": "dblclick", "index": 2, "column": -1})
			p.send(map[string]any{"type": "dblclick", "index": 9, "column": 0})
			p.waitClosed()
		},
	}

	src := `
const v = await ui.form({ title: "窓", fields: [
  { name: "src", label: "フォルダ", type: "folder", required: true },
  { name: "n", type: "number" },
  { name: "mode", type: "select", options: ["顧客別", "商品別"] },
  { name: "day", type: "date", default: new Date(2026, 3, 1) },
  { name: "inputs", type: "files", accept: ".xlsx" },
]});
console.log("1", v.src.name, v.n, v.mode, v.day.getDate(), v.inputs.map((h) => h.name).join());

const v2 = await ui.form({ title: "実行", fields: [{ name: "n", type: "number" }], async onSubmit(values, ctx) {
  for (const i of ctx.withProgress([1, 2, 3])) ctx.log("item " + i);
  return fs.file("data/a.xlsx");
}});
console.log("2", v2.n);

const v3 = await ui.form({ title: "中止", fields: [], async onSubmit(values, ctx) {
  for (let i = 0; i < 200 && !ctx.cancelled; i++) await sleep(5);
}});
console.log("3", v3 !== null);

await ui.form({ fields: [], onSubmit() { throw new Error("失敗"); } });

const rows = [{ 名前: "a", 数: 1, 日: new Date(2026, 3, 1) }, { 名前: "b", 数: 2 }, { 名前: "c", 日: new Date(2026, 3, 1, 12, 30) }];
const picked = await ui.table(rows, { title: "表", selectable: true });
console.log("5", picked.map((r) => r.名前).join());
console.log("csv", JSON.stringify(fs.readText("out.csv")));
console.log("xlsx", workbooks.open("out.xlsx").sheets[0].toArrays().length);

const view = ui.table(rows, { onDoubleClick(e) {
  console.log("dbl", e.index, e.row.名前, e.column, e.key, e.value);
  if (e.column === null) e.close();
}});
await view.closed;
console.log("6 closed");
`
	p := writeScript(t, "w.js", src)
	var out, errOut bytes.Buffer
	var code int
	finished := make(chan struct{})
	go func() {
		defer close(finished)
		code, err = Run(Options{ScriptPath: p, UI: u, Stdout: &out, Stderr: &errOut})
	}()
	select {
	case <-finished:
	case <-time.After(20 * time.Second):
		t.Fatalf("script did not finish\nstdout:\n%s\nlog:\n%s", out.String(), log.String())
	}
	u.wg.Wait()
	if code != 0 || err != nil {
		t.Fatalf("exit %d: %v\nstdout:\n%s\nstderr:\n%s\nlog:\n%s", code, err, out.String(), errOut.String(), log.String())
	}
	want := "1 data 2 商品別 6 a.xlsx\n2 3\n3 true\n5 a,c\n" +
		"csv \"名前,数,日\\r\\nc,,2026/04/01 12:30:00\\r\\na,1,2026/04/01\\r\\n\"" + "\nxlsx 4\ndbl 1 b 数 数 2\ndbl 2 c null null undefined\n6 closed\n"
	if out.String() != want {
		t.Errorf("stdout = %q\nwant     %q", out.String(), want)
	}
	if !strings.Contains(log.String(), "showItemInFolder([") {
		t.Errorf("open folder not requested:\n%s", log.String())
	}
	for _, u := range u.urls {
		if !strings.HasPrefix(u, ui.AssetOrigin+ui.BuiltinPrefix) {
			t.Errorf("window url %s", u)
		}
	}
}

// attachUI opens windows that call the script's "dropped" with two
// attached paths, as when a folder and a file are dropped on a page.
type attachUI struct {
	*ui.Mock
	files []string
}

func (u attachUI) OpenWindow(o ui.WindowOptions) (ui.Window, error) {
	w := &slowWindow{u: &slowUI{Mock: u.Mock}, o: o}
	go func() {
		o.OnMessage(`{"xljs":"ready"}`, nil)
		o.OnMessage(`{"xljs":"invoke","id":1,"name":"dropped","args":[[{"$attach":0},{"$attach":1},{"$attach":2}]]}`, u.files)
	}()
	return w, nil
}

func TestDroppedFolder(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "a.xlsx")
	if err := os.WriteFile(file, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	m, _ := ui.NewMockFromJSON("", io.Discard)
	src := "// ==xljs==\n// @mode app\n// ==/xljs==\n" +
		"export function dropped(hs) { console.log(hs.map((h) => h && h.kind).join()); app.main.close(); }\n"
	var out bytes.Buffer
	code, err := Run(Options{ScriptPath: writeScript(t, "drop.js", src), UI: attachUI{m, []string{dir, file, ""}}, Stdout: &out, Stderr: io.Discard})
	if code != 0 || err != nil || out.String() != "directory,file,\n" {
		t.Fatalf("exit %d, %v, stdout %q", code, err, out.String())
	}
}
