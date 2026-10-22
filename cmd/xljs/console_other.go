//go:build !windows

package main

func setupConsole() *consoleState { return &consoleState{has: true} }
