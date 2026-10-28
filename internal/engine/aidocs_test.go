package engine

import (
	"bytes"
	"os"
	"path/filepath"
	"regexp"
	"testing"
	"unicode/utf8"
)

// promptLimit keeps docs/ai/prompt.md short enough to paste into chats
// whose input box has a size limit (Microsoft 365 Copilot: 16,000
// characters) with room left for the request.
const promptLimit = 15000

// copilotLimit is the size limit of the instructions of a Microsoft 365
// Copilot agent (docs/ai/copilot/instructions.md).
const copilotLimit = 8000

var jsBlock = regexp.MustCompile("(?s)```js\n(.*?)```")

// TestAIDocs checks that the code in the documents for AI chats
// (docs/ai) is valid syntax, since the models copy it as is.
func TestAIDocs(t *testing.T) {
	dir := filepath.Join("..", "..", "docs", "ai")
	for _, name := range []string{"prompt.md", "reference.md", "copilot/instructions.md", "copilot/skills/xljs-reference/SKILL.md", "copilot/skills/vba-port/SKILL.md"} {
		src, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		// A Windows checkout may have CRLF line ends.
		src = bytes.ReplaceAll(src, []byte("\r\n"), []byte("\n"))
		limit := map[string]int{"prompt.md": promptLimit, "copilot/instructions.md": copilotLimit}[name]
		if n := utf8.RuneCount(src); limit > 0 && n > limit {
			t.Errorf("%s is %d characters; keep it within %d", name, n, limit)
		}
		blocks := jsBlock.FindAllSubmatch(src, -1)
		if len(blocks) == 0 {
			t.Errorf("%s: no js code blocks", name)
		}
		for i, m := range blocks {
			entry := filepath.Join(t.TempDir(), "block.js")
			if err := os.WriteFile(entry, m[1], 0o644); err != nil {
				t.Fatal(err)
			}
			if _, err := Bundle(entry); err != nil {
				t.Errorf("%s: code block %d: %v", name, i+1, err)
			}
		}
	}
}

// TestAIReferenceSkill checks that the reference skill for Copilot agents
// is a copy of docs/ai/reference.md.
func TestAIReferenceSkill(t *testing.T) {
	dir := filepath.Join("..", "..", "docs", "ai")
	want, err := os.ReadFile(filepath.Join(dir, "reference.md"))
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(dir, "copilot", "skills", "xljs-reference", "SKILL.md"))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(bytes.ReplaceAll(got, []byte("\r\n"), []byte("\n")), bytes.ReplaceAll(want, []byte("\r\n"), []byte("\n"))) {
		t.Error("copilot/skills/xljs-reference/SKILL.md differs from reference.md; copy reference.md to it")
	}
}
