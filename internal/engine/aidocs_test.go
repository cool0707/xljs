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

var jsBlock = regexp.MustCompile("(?s)```js\n(.*?)```")

// TestAIDocs checks that the code in the documents for AI chats
// (docs/ai) is valid syntax, since the models copy it as is.
func TestAIDocs(t *testing.T) {
	dir := filepath.Join("..", "..", "docs", "ai")
	for _, name := range []string{"prompt.md", "reference.md"} {
		src, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		// A Windows checkout may have CRLF line ends.
		src = bytes.ReplaceAll(src, []byte("\r\n"), []byte("\n"))
		if name == "prompt.md" {
			if n := utf8.RuneCount(src); n > promptLimit {
				t.Errorf("prompt.md is %d characters; keep it within %d", n, promptLimit)
			}
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
