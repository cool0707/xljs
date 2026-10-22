// Command xljs runs JavaScript/TypeScript scripts that read and write Excel
// and CSV files, with file pickers, dialogs and progress indicators.
package main

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"strings"

	"github.com/cool0707/xljs/internal/engine"
	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/pack"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/cool0707/xljs/types"
)

// version is set with -ldflags "-X main.version=v1.2.3".
var version = ""

func init() {
	// Dialogs (COM, STA) must be created on a single OS thread; keep the
	// main goroutine on the main thread for the lifetime of the process.
	runtime.LockOSThread()
}

func getVersion() string {
	if version != "" {
		return version
	}
	if bi, ok := debug.ReadBuildInfo(); ok && bi.Main.Version != "" && bi.Main.Version != "(devel)" {
		return bi.Main.Version
	}
	return "dev"
}

func usage() string { return i18n.T("cli.usage", strings.Join(i18n.Available(), "|")) }

func main() {
	os.Exit(run())
}

func run() int {
	con := setupConsole()
	defer con.finish()

	signals := make(chan os.Signal, 2)
	signal.Notify(signals, os.Interrupt)
	go func() {
		for range signals {
			n := ui.NotifyInterrupt()
			if !ui.ProgressActive() || n >= 2 {
				fmt.Fprintln(os.Stderr, "\n"+i18n.T("cli.interrupted"))
				os.Exit(130)
			}
			fmt.Fprintln(os.Stderr, "\n"+i18n.T("cli.cancelRequested"))
		}
	}()

	// A packed exe runs its embedded script; all arguments go to the script.
	if payload, err := pack.ReadSelf(); err == nil && payload != nil {
		u, err := chooseUI(os.Getenv("XLJS_UI"), os.Getenv("XLJS_UI_ANSWERS"))
		if err != nil {
			return con.fail(nil, err)
		}
		exe, _ := os.Executable()
		script := filepath.Join(filepath.Dir(exe), payload.Name)
		return runScript(con, u, engine.Options{ScriptPath: script, Code: payload.Code, Args: os.Args[1:]})
	}

	// Global options come first; the first other argument is either a
	// subcommand or the script.
	fset := flag.NewFlagSet("xljs", flag.ContinueOnError)
	fset.SetOutput(io.Discard)
	uiMode := fset.String("ui", os.Getenv("XLJS_UI"), "")
	answers := fset.String("ui-answers", os.Getenv("XLJS_UI_ANSWERS"), "")
	lang := fset.String("lang", "", "")
	showHelp := fset.Bool("help", false, "")
	fset.BoolVar(showHelp, "h", false, "")
	showVersion := fset.Bool("version", false, "")
	fset.BoolVar(showVersion, "v", false, "")
	if err := fset.Parse(os.Args[1:]); err != nil {
		fmt.Fprint(os.Stderr, usage())
		return con.fail(nil, err)
	}
	if *lang != "" && !i18n.SetLanguage(*lang) {
		return con.fail(nil, i18n.Err("cli.unknownLang", *lang, strings.Join(i18n.Available(), ", ")))
	}
	rest := fset.Args()
	switch {
	case *showVersion:
		fmt.Println("xljs", getVersion())
		return 0
	case *showHelp:
		fmt.Print(usage())
		return 0
	}
	if len(rest) > 0 {
		switch rest[0] {
		case "version":
			fmt.Println("xljs", getVersion())
			return 0
		case "help", "/?":
			fmt.Print(usage())
			return 0
		case "pack":
			return cmdPack(con, rest[1:])
		case "check":
			return cmdCheck(con, rest[1:])
		case "init":
			return cmdInit(con, rest[1:])
		}
	}

	u, err := chooseUI(*uiMode, *answers)
	if err != nil {
		return con.fail(nil, err)
	}

	if len(rest) == 0 {
		// Double-clicked without a script: let the user pick one.
		if u.Name() == "native" {
			paths := u.OpenFiles(ui.PickerOptions{
				Title:   i18n.T("cli.selectScript"),
				Filters: []ui.FileFilter{{Description: i18n.T("cli.scripts"), Patterns: []string{"*.js", "*.mjs", "*.ts"}}},
			})
			if len(paths) == 0 {
				return 0
			}
			rest = paths
		} else {
			fmt.Fprint(os.Stderr, usage())
			return 2
		}
	}
	return runScript(con, u, engine.Options{ScriptPath: rest[0], Args: rest[1:]})
}

func chooseUI(mode, answers string) (ui.UI, error) {
	switch strings.ToLower(mode) {
	case "", "auto":
		if ui.NativeAvailable() {
			return ui.NewNative()
		}
		return ui.NewConsole(), nil
	case "native":
		return ui.NewNative()
	case "console", "cui", "cli":
		return ui.NewConsole(), nil
	case "mock":
		return ui.NewMock(answers, os.Stderr)
	}
	return nil, i18n.Err("cli.unknownUI", mode)
}

func runScript(con *consoleState, u ui.UI, o engine.Options) int {
	o.UI = u
	o.Version = getVersion()
	code, err := engine.Run(o)
	if err != nil {
		return con.fail(u, err)
	}
	return code
}

func cmdCheck(con *consoleState, args []string) int {
	if len(args) != 1 {
		fmt.Fprintln(os.Stderr, i18n.T("cli.usageCheck"))
		return 2
	}
	if _, err := engine.Bundle(args[0]); err != nil {
		return con.fail(nil, err)
	}
	fmt.Println(i18n.T("cli.checkOK"))
	return 0
}

func cmdPack(con *consoleState, args []string) int {
	fset := flag.NewFlagSet("pack", flag.ContinueOnError)
	out := fset.String("o", "", "")
	var script string
	// Allow flags before or after the script name.
	for len(args) > 0 {
		if err := fset.Parse(args); err != nil {
			return con.fail(nil, err)
		}
		args = fset.Args()
		if len(args) > 0 {
			if script != "" {
				fmt.Fprintln(os.Stderr, i18n.T("cli.usagePack"))
				return 2
			}
			script = args[0]
			args = args[1:]
		}
	}
	if script == "" {
		fmt.Fprintln(os.Stderr, i18n.T("cli.usagePack"))
		return 2
	}
	code, err := engine.Bundle(script)
	if err != nil {
		return con.fail(nil, err)
	}
	if *out == "" {
		base := strings.TrimSuffix(filepath.Base(script), filepath.Ext(script))
		ext := ""
		if runtime.GOOS == "windows" {
			ext = ".exe"
		}
		*out = filepath.Join(filepath.Dir(script), base+ext)
	}
	name := filepath.Base(script)
	if err := pack.Write(*out, name, code); err != nil {
		return con.fail(nil, err)
	}
	fmt.Println(i18n.T("cli.created", *out))
	return 0
}

func cmdInit(con *consoleState, args []string) int {
	dir := "."
	if len(args) > 0 {
		dir = args[0]
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return con.fail(nil, err)
	}
	dts := filepath.Join(dir, "xljs.d.ts")
	if err := os.WriteFile(dts, []byte(types.DTS), 0o644); err != nil {
		return con.fail(nil, err)
	}
	fmt.Println(i18n.T("cli.created", dts))
	jsconfig := filepath.Join(dir, "jsconfig.json")
	if _, err := os.Stat(jsconfig); errors.Is(err, os.ErrNotExist) {
		cfg := `{
  "compilerOptions": {
    "target": "ES2023",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "moduleDetection": "force",
    "allowImportingTsExtensions": true,
    "noEmit": true,
    "checkJs": true,
    "lib": ["ES2023"]
  },
  "include": ["*.js", "*.ts", "**/*.js", "**/*.ts", "xljs.d.ts"]
}
`
		if err := os.WriteFile(jsconfig, []byte(cfg), 0o644); err != nil {
			return con.fail(nil, err)
		}
		fmt.Println(i18n.T("cli.created", jsconfig))
	}
	return 0
}
