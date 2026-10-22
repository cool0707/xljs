package pack

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
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
	if err := WriteFrom(base, out, "script.js", []byte("console.log(1)")); err != nil {
		t.Fatal(err)
	}
	p, err := Read(out)
	if err != nil || p == nil {
		t.Fatalf("read: %v %v", p, err)
	}
	if p.Name != "script.js" || string(p.Code) != "console.log(1)" {
		t.Fatalf("payload = %+v", p)
	}
	// Re-packing a packed exe replaces the payload instead of stacking.
	out2 := filepath.Join(dir, "out2.exe")
	if err := WriteFrom(out, out2, "b.js", []byte("2")); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(out2)
	if !bytes.HasPrefix(data, []byte("MZ fake executable")) || bytes.Contains(data, []byte("console.log(1)")) {
		t.Fatalf("unexpected content %q", data)
	}
	p, _ = Read(out2)
	if p.Name != "b.js" || string(p.Code) != "2" {
		t.Fatalf("payload = %+v", p)
	}
}
