//go:build !windows

package ui

func newThread(UI) uiThread { return newChanThread() }
