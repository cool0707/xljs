//go:build !windows

package ui

import (
	"errors"

	"github.com/cool0707/xljs/internal/i18n"
)

// NewNative returns the native UI. It is only available on Windows.
func NewNative() (UI, error) {
	return nil, errors.New(i18n.S("ネイティブ UI は Windows でのみ使えます", "native UI is only available on Windows"))
}

// NativeAvailable reports whether NewNative can succeed.
func NativeAvailable() bool { return false }
