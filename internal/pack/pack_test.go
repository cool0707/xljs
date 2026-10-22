package pack

import (
	"bytes"
	"encoding/binary"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/cool0707/xljs/internal/meta"
)

func TestWriteRead(t *testing.T) {
	dir := t.TempDir()
	base := filepath.Join(dir, "base.exe")
	if err := os.WriteFile(base, []byte("MZ fake executable"), 0o755); err != nil {
		t.Fatal(err)
	}
	if p, err := Read(base); err != nil || p != nil {
		t.Fatalf("plain file: %v %v", p, err)
	}
	out := filepath.Join(dir, "out.exe")
	if err := WriteFrom(base, out, &Payload{Name: "script.js", Code: []byte("console.log(1)")}); err != nil {
		t.Fatal(err)
	}
	p, err := Read(out)
	if err != nil || p == nil {
		t.Fatalf("read: %v %v", p, err)
	}
	if p.Name != "script.js" || string(p.Code) != "console.log(1)" || p.Meta != nil || p.Assets != nil {
		t.Fatalf("payload = %+v", p)
	}
	// Re-packing a packed exe replaces the payload instead of stacking.
	out2 := filepath.Join(dir, "out2.exe")
	if err := WriteFrom(out, out2, &Payload{Name: "b.js", Code: []byte("2")}); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(out2)
	if !bytes.HasPrefix(data, []byte("MZ fake executable")) || bytes.Count(data, []byte("XLJSPAK2")) != 1 {
		t.Fatalf("unexpected content %q", data)
	}
	p, _ = Read(out2)
	if p.Name != "b.js" || string(p.Code) != "2" {
		t.Fatalf("payload = %+v", p)
	}
}

func TestAssets(t *testing.T) {
	dir := t.TempDir()
	for _, f := range []string{"app.js", "ui/index.html", "ui/img/logo.png", "secret.txt"} {
		p := filepath.Join(dir, filepath.FromSlash(f))
		os.MkdirAll(filepath.Dir(p), 0o755)
		os.WriteFile(p, []byte("<"+f+">"), 0o644)
	}
	base := filepath.Join(dir, "base.exe")
	os.WriteFile(base, []byte("MZ"), 0o755)
	m := &meta.Meta{Mode: "app", HTML: "ui/index.html", Assets: []string{"ui"}, Title: "売上", Width: 800, Height: 600}
	out := filepath.Join(dir, "out.exe")
	if err := WriteFrom(base, out, &Payload{Name: "app.js", Code: []byte("x"), Meta: m, Assets: m.AssetFS(dir)}); err != nil {
		t.Fatal(err)
	}
	p, err := Read(out)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(p.Meta, m) {
		t.Fatalf("meta = %+v", p.Meta)
	}
	for _, f := range []string{"ui/index.html", "ui/img/logo.png"} {
		if b, err := fs.ReadFile(p.Assets, f); err != nil || string(b) != "<"+f+">" {
			t.Errorf("%s: %q %v", f, b, err)
		}
	}
	for _, f := range []string{"secret.txt", "app.js", "../manifest.json"} {
		if _, err := fs.ReadFile(p.Assets, f); err == nil {
			t.Errorf("%s should not be embedded", f)
		}
	}

	// A missing @html file is an error.
	m2 := &meta.Meta{HTML: "missing.html"}
	if err := WriteFrom(base, out, &Payload{Name: "app.js", Code: []byte("x"), Meta: m2, Assets: m2.AssetFS(dir)}); err == nil {
		t.Fatal("expected an error for a missing @html file")
	}
}

func TestReadV1(t *testing.T) {
	dir := t.TempDir()
	exe := filepath.Join(dir, "old.exe")
	name, code := "old.js", "console.log('v1')"
	payload := binary.LittleEndian.AppendUint32(nil, uint32(len(name)))
	payload = append(payload, name...)
	payload = append(payload, code...)
	data := append([]byte("MZ old"), payload...)
	data = binary.LittleEndian.AppendUint64(data, uint64(len(payload)))
	data = append(data, "XLJSPAK1"...)
	os.WriteFile(exe, data, 0o755)
	p, err := Read(exe)
	if err != nil || p == nil || p.Name != name || string(p.Code) != code {
		t.Fatalf("v1: %+v %v", p, err)
	}
	// Packing over a v1 exe strips the old payload.
	out := filepath.Join(dir, "new.exe")
	if err := WriteFrom(exe, out, &Payload{Name: "n.js", Code: []byte("1")}); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(out)
	if !bytes.HasPrefix(got, []byte("MZ old")) || bytes.Contains(got, []byte("XLJSPAK1")) {
		t.Fatalf("v1 payload not stripped: %q", got)
	}
}
