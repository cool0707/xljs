//go:build !windows

package ui

import (
	"os/exec"
	"runtime"
)

// ShellShowInFolder opens the file manager on the folders of the files
// (macOS selects the files; other systems just open the folder).
func ShellShowInFolder(paths []string) error {
	if err := checkPaths(paths); err != nil {
		return err
	}
	order, groups, err := groupByDir(paths)
	if err != nil {
		return err
	}
	for _, dir := range order {
		var cmd *exec.Cmd
		if runtime.GOOS == "darwin" {
			cmd = exec.Command("open", append([]string{"-R"}, groups[dir]...)...)
		} else {
			cmd = exec.Command("xdg-open", dir)
		}
		if err := cmd.Start(); err != nil {
			return err
		}
	}
	return nil
}

// ShellOpen opens a file, folder or URL with its default application.
func ShellOpen(target string) error {
	name := "xdg-open"
	if runtime.GOOS == "darwin" {
		name = "open"
	}
	return exec.Command(name, target).Start()
}
