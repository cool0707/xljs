package ui

import (
	"errors"
	"os"
	"path/filepath"

	"github.com/cool0707/xljs/internal/i18n"
)

// checkPaths makes the paths absolute and verifies that they exist.
func checkPaths(paths []string) error {
	if len(paths) == 0 {
		return errors.New(i18n.S("表示するファイルが指定されていません", "no files to show"))
	}
	for _, p := range paths {
		if _, err := os.Stat(p); err != nil {
			return err
		}
	}
	return nil
}

// groupByDir returns absolute paths grouped by parent folder, keeping the
// order in which folders first appear.
func groupByDir(paths []string) ([]string, map[string][]string, error) {
	var order []string
	groups := map[string][]string{}
	for _, p := range paths {
		abs, err := filepath.Abs(p)
		if err != nil {
			return nil, nil, err
		}
		dir := filepath.Dir(abs)
		if _, ok := groups[dir]; !ok {
			order = append(order, dir)
		}
		groups[dir] = append(groups[dir], abs)
	}
	return order, groups, nil
}
