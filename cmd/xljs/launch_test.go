package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/pack"
	"github.com/cool0707/xljs/internal/ui"
)

// launchWith runs the start screen with the mock answers.
func launchWith(t *testing.T, answers map[string]any) (int, string) {
	t.Helper()
	doc, _ := json.Marshal(answers)
	var log strings.Builder
	u, err := ui.NewMockFromJSON(string(doc), &log)
	if err != nil {
		t.Fatal(err)
	}
	code := launch(&consoleState{has: true}, u, u, false)
	return code, log.String()
}

func TestLaunch(t *testing.T) {
	dir := t.TempDir()
	script := filepath.Join(dir, "hello.js")
	os.WriteFile(script, []byte(`ui.alert("こんにちは");`), 0o644)

	t.Run("cancel", func(t *testing.T) {
		if code, log := launchWith(t, nil); code != 0 || !strings.Contains(log, "-> -1") {
			t.Errorf("exit %d\n%s", code, log)
		}
	})
	t.Run("run", func(t *testing.T) {
		code, log := launchWith(t, map[string]any{"choose": []int{launchRun}, "showOpenFilePicker": [][]string{{script}}})
		if code != 0 || !strings.Contains(log, "こんにちは") {
			t.Errorf("exit %d\n%s", code, log)
		}
	})
	t.Run("pack", func(t *testing.T) {
		out := filepath.Join(dir, "hello.exe")
		code, log := launchWith(t, map[string]any{
			"choose": []int{launchPack}, "showOpenFilePicker": [][]string{{script}}, "showSaveFilePicker": []string{out},
		})
		if code != 0 {
			t.Fatalf("exit %d\n%s", code, log)
		}
		p, err := pack.Read(out)
		if err != nil || p == nil || p.Name != "hello.js" {
			t.Errorf("packed: %v %v", p, err)
		}
	})
	t.Run("vba", func(t *testing.T) {
		data, err := os.ReadFile(filepath.Join("..", "..", "testdata", "scripts", "macro.xlsm"))
		if err != nil {
			t.Fatal(err)
		}
		book := filepath.Join(dir, "売上.xlsm")
		os.WriteFile(book, data, 0o644)
		code, log := launchWith(t, map[string]any{"choose": []int{launchVBA}, "showOpenFilePicker": [][]string{{book}}})
		if code != 0 {
			t.Fatalf("exit %d\n%s", code, log)
		}
		if _, err := os.Stat(filepath.Join(dir, "売上_VBA.txt")); err != nil {
			t.Error(err)
		}
	})
}
