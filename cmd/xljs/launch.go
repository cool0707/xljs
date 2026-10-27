package main

import (
	"path/filepath"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
)

// What the start screen offers, in its order.
const (
	launchRun = iota
	launchDebug
	launchPack
	launchVBA
)

// launch is the start screen of xljs started without a script
// (double-clicked): it asks whether to run, debug or pack a script, or to
// extract the VBA code of macro workbooks, and asks for the files.
func launch(con *consoleState, u ui.UI, c ui.Chooser, explicitLang bool) int {
	choice := c.Choose("xljs", i18n.T("cli.launchMessage"), []ui.Choice{
		{Label: i18n.T("cli.launchRun"), Note: i18n.T("cli.launchRunNote")},
		{Label: i18n.T("cli.launchDebug"), Note: i18n.T("cli.launchDebugNote")},
		{Label: i18n.T("cli.launchPack"), Note: i18n.T("cli.launchPackNote")},
		{Label: i18n.T("cli.launchVBA"), Note: i18n.T("cli.launchVBANote")},
	})
	switch choice {
	case launchRun, launchDebug:
		script := pickScript(u, i18n.T("cli.selectScript"))
		if script == "" {
			return 0
		}
		debug := ""
		if choice == launchDebug {
			debug = "script"
		}
		return start(con, u, []string{script}, explicitLang, debug)
	case launchPack:
		script := pickScript(u, i18n.T("cli.selectScriptToPack"))
		if script == "" {
			return 0
		}
		return launchPackScript(con, u, script)
	case launchVBA:
		books := u.OpenFiles(ui.PickerOptions{
			Title:    i18n.T("cli.selectMacroBooks"),
			Multiple: true,
			Filters:  []ui.FileFilter{{Description: i18n.T("cli.macroBooks"), Patterns: macroPatterns}},
		})
		if len(books) == 0 {
			return 0
		}
		return dropVBA(con, u, books)
	}
	return 0
}

// pickScript asks for a script; "" when cancelled.
func pickScript(u ui.UI, title string) string {
	paths := u.OpenFiles(ui.PickerOptions{
		Title:   title,
		Filters: []ui.FileFilter{{Description: i18n.T("cli.scripts"), Patterns: []string{"*.js", "*.mjs", "*.ts"}}},
	})
	if len(paths) == 0 {
		return ""
	}
	return paths[0]
}

// launchPackScript asks where to save the exe for the script, creates it
// and offers to show it in its folder.
func launchPackScript(con *consoleState, u ui.UI, script string) int {
	abs, _ := filepath.Abs(packOutput(script))
	out, ok := u.SaveFile(ui.PickerOptions{
		Title:         i18n.T("cli.packSaveTitle"),
		StartIn:       filepath.Dir(abs),
		SuggestedName: filepath.Base(abs),
		Filters:       []ui.FileFilter{{Description: i18n.T("cli.programs"), Patterns: []string{"*.exe"}}},
	})
	if !ok {
		return 0
	}
	if err := packScript(script, out); err != nil {
		return con.fail(u, err)
	}
	if u.Confirm("xljs", i18n.T("cli.packed", out)) {
		if err := u.ShowInFolder([]string{out}); err != nil {
			return con.fail(u, err)
		}
	}
	return 0
}
