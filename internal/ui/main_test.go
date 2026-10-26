package ui

import (
	"os"
	"testing"
)

// TestMain gives the tests a WebView2 user data folder of their own:
// other test binaries (the engine package) use WebView2 at the same time.
func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "xljs-webview2-")
	if err == nil {
		os.Setenv(WebView2DataEnv, dir)
	}
	code := m.Run()
	if err == nil {
		os.RemoveAll(dir)
	}
	os.Exit(code)
}
