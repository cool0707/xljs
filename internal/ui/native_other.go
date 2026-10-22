//go:build !windows

package ui

import "errors"

// NewNative returns the native UI. It is only available on Windows.
func NewNative() (UI, error) {
	return nil, errors.New("native UI is only available on Windows")
}

// NativeAvailable reports whether NewNative can succeed.
func NativeAvailable() bool { return false }
