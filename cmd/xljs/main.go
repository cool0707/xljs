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

const usage = `xljs - Excel/CSV スクリプトランナー

使い方:
  xljs [オプション] <script.js|script.ts> [引数...]
  xljs pack <script> [-o 出力.exe]   スクリプトを埋め込んだ exe を作成
  xljs check <script>                構文と import を検査（実行しない）
  xljs init [フォルダ]               型定義 (xljs.d.ts) と jsconfig.json を作成
  xljs version                       バージョンを表示

オプション:
  --ui=auto|native|console|mock      UI の種類（既定: auto）
  --ui-answers=<file.json>           mock UI の応答ファイル

環境変数 XLJS_UI / XLJS_UI_ANSWERS でも UI を指定できます（埋め込み exe 向け）。
`

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
				fmt.Fprintln(os.Stderr, "\n中断しました")
				os.Exit(130)
			}
			fmt.Fprintln(os.Stderr, "\nキャンセルを要求しました（もう一度 Ctrl+C で強制終了）")
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

	args := os.Args[1:]
	if len(args) > 0 {
		switch args[0] {
		case "version", "--version", "-v":
			fmt.Println("xljs", getVersion())
			return 0
		case "help", "--help", "-h", "/?":
			fmt.Print(usage)
			return 0
		case "pack":
			return cmdPack(con, args[1:])
		case "check":
			return cmdCheck(con, args[1:])
		case "init":
			return cmdInit(con, args[1:])
		}
	}

	fset := flag.NewFlagSet("xljs", flag.ContinueOnError)
	fset.SetOutput(io.Discard)
	uiMode := fset.String("ui", os.Getenv("XLJS_UI"), "")
	answers := fset.String("ui-answers", os.Getenv("XLJS_UI_ANSWERS"), "")
	if err := fset.Parse(args); err != nil {
		fmt.Fprint(os.Stderr, usage)
		return con.fail(nil, err)
	}
	rest := fset.Args()

	u, err := chooseUI(*uiMode, *answers)
	if err != nil {
		return con.fail(nil, err)
	}

	if len(rest) == 0 {
		// Double-clicked without a script: let the user pick one.
		if u.Name() == "native" {
			paths := u.OpenFiles(ui.PickerOptions{
				Title:   "実行するスクリプトを選択",
				Filters: []ui.FileFilter{{Description: "スクリプト", Patterns: []string{"*.js", "*.mjs", "*.ts"}}},
			})
			if len(paths) == 0 {
				return 0
			}
			rest = paths
		} else {
			fmt.Fprint(os.Stderr, usage)
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
	return nil, fmt.Errorf("unknown --ui value %q (auto, native, console, mock)", mode)
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
		fmt.Fprintln(os.Stderr, "使い方: xljs check <script>")
		return 2
	}
	if _, err := engine.Bundle(args[0]); err != nil {
		return con.fail(nil, err)
	}
	fmt.Println("OK")
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
				fmt.Fprintln(os.Stderr, "使い方: xljs pack <script> [-o 出力.exe]")
				return 2
			}
			script = args[0]
			args = args[1:]
		}
	}
	if script == "" {
		fmt.Fprintln(os.Stderr, "使い方: xljs pack <script> [-o 出力.exe]")
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
	fmt.Println("作成しました:", *out)
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
	fmt.Println("作成しました:", dts)
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
		fmt.Println("作成しました:", jsconfig)
	}
	return 0
}
