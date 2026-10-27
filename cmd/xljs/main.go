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
	"github.com/cool0707/xljs/internal/meta"
	"github.com/cool0707/xljs/internal/pack"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/cool0707/xljs/types"
)

// version is set with -ldflags "-X main.version=v1.2.3".
var version = ""

func init() {
	// The main thread is the UI thread: dialogs (COM, STA) and windows are
	// created there, while scripts run on their own thread (see ui.Run).
	// Keep the main goroutine on it for the lifetime of the process.
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
		m := payload.Meta
		if m == nil {
			m = &meta.Meta{}
		}
		applyLang(m, false)
		assets := payload.Assets
		if assets == nil {
			assets = m.AssetFS(filepath.Dir(exe)) // nothing visible
		}
		o := engine.Options{ScriptPath: script, Code: payload.Code, Args: os.Args[1:], Meta: m, Assets: assets}
		useScriptDir(con, &o)
		if err := setDebug(&o, os.Getenv("XLJS_DEBUG"), u); err != nil {
			return con.fail(nil, err)
		}
		return runScript(con, u, o)
	}

	// Global options come first; the first other argument is either a
	// subcommand or the script.
	fset := flag.NewFlagSet("xljs", flag.ContinueOnError)
	fset.SetOutput(io.Discard)
	uiMode := fset.String("ui", os.Getenv("XLJS_UI"), "")
	answers := fset.String("ui-answers", os.Getenv("XLJS_UI_ANSWERS"), "")
	lang := fset.String("lang", "", "")
	debug := &debugFlag{value: os.Getenv("XLJS_DEBUG")}
	fset.Var(debug, "debug", "")
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
		case "vba":
			return cmdVBA(con, rest[1:])
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
				Title: i18n.T("cli.selectScript"),
				Filters: []ui.FileFilter{
					{Description: i18n.T("cli.scripts"), Patterns: []string{"*.js", "*.mjs", "*.ts"}},
					{Description: i18n.T("cli.macroBooks"), Patterns: macroPatterns},
				},
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
	// Workbooks instead of a script: write out their VBA code.
	if isWorkbook(rest[0]) {
		return dropVBA(con, u, rest)
	}
	m, err := readMeta(rest[0])
	if err != nil {
		return con.fail(u, err)
	}
	applyLang(m, *lang != "")
	o := engine.Options{ScriptPath: rest[0], Args: rest[1:], Meta: m}
	useScriptDir(con, &o)
	if err := setDebug(&o, debug.value, u); err != nil {
		return con.fail(u, err)
	}
	return runScript(con, u, o)
}

// useScriptDir makes the script's folder the working folder when xljs was
// started from Explorer (double-clicked, or a file dropped onto the exe or
// the script), where the working folder is whatever Explorer chose (often
// System32 or the user folder), so that relative paths in the script mean
// "next to the script". Arguments naming existing files are made absolute
// first. From a terminal the working folder is left as it is.
func useScriptDir(con *consoleState, o *engine.Options) {
	if !con.fromExplorer() {
		return
	}
	for i, a := range o.Args {
		if _, err := os.Stat(a); err == nil {
			if abs, err := filepath.Abs(a); err == nil {
				o.Args[i] = abs
			}
		}
	}
	abs, err := filepath.Abs(o.ScriptPath)
	if err != nil {
		return
	}
	o.ScriptPath = abs
	os.Chdir(filepath.Dir(abs))
}

// debugFlag is --debug, --debug=script or --debug=window (see setDebug).
type debugFlag struct{ value string }

func (f *debugFlag) String() string     { return f.value }
func (f *debugFlag) Set(s string) error { f.value = s; return nil }
func (f *debugFlag) IsBoolFlag() bool   { return true }

// setDebug applies the --debug (XLJS_DEBUG) value: "script" (also true, 1,
// on) runs the script in the debugger window, which needs the native UI;
// "window" only enables the developer tools of the script's windows.
func setDebug(o *engine.Options, value string, u ui.UI) error {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "", "0", "false", "no", "off":
	case "1", "true", "yes", "on", "script":
		o.Debug = true
		if u.Name() == "native" {
			o.DebugScript = true
		} else {
			fmt.Fprintln(os.Stderr, i18n.T("cli.debugNeedsWindows"))
		}
	case "window":
		o.Debug = true
	default:
		return i18n.Err("cli.unknownDebug", value)
	}
	return nil
}

// readMeta parses the metadata block of a script and prints its warnings.
func readMeta(script string) (*meta.Meta, error) {
	m, warnings, err := meta.ParseFile(script)
	for _, w := range warnings {
		fmt.Fprintln(os.Stderr, w)
	}
	return m, err
}

// applyLang switches to the script's @lang unless the language was chosen
// explicitly (--lang or XLJS_LANG).
func applyLang(m *meta.Meta, explicit bool) {
	if m.Lang == "" || explicit || os.Getenv("XLJS_LANG") != "" {
		return
	}
	if !i18n.SetLanguage(m.Lang) {
		fmt.Fprintln(os.Stderr, i18n.T("cli.unknownMetaLang", m.Lang, strings.Join(i18n.Available(), ", ")))
	}
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

// runScript runs the script on its own thread; this (main) thread serves
// the UI meanwhile.
func runScript(con *consoleState, u ui.UI, o engine.Options) int {
	o.Version = getVersion()
	var code int
	var err error
	ui.Run(u, func(su ui.UI) {
		o.UI = su
		code, err = engine.Run(o)
	})
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
	if _, err := readMeta(args[0]); err != nil {
		return con.fail(nil, err)
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
	m, err := readMeta(script)
	if err != nil {
		return con.fail(nil, err)
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
	p := &pack.Payload{Name: filepath.Base(script), Code: code, Meta: m, Assets: m.AssetFS(filepath.Dir(script))}
	if err := pack.Write(*out, p); err != nil {
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
