//go:build !windows

package ui

import (
	"github.com/cool0707/xljs/internal/i18n"
)

// NewNative returns the native UI. It is only available on Windows.
func NewNative() (UI, error) {
	return nil, i18n.Err("ui.nativeUnavailable")
}

// NativeAvailable reports whether NewNative can succeed.
func NativeAvailable() bool { return false }
