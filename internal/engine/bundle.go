package engine

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/evanw/esbuild/pkg/api"
)

// The bundled script is wrapped in an async function so that top-level
// await works. The program evaluates to that function; the engine calls it
// (so no extra wrapper frame appears in stack traces) with a function that
// receives the entry's exports (see exportsCall).
const (
	bundleBanner = `(async (` + exportsParam + `) => {"use strict";`
	bundleFooter = `})`
	exportsParam = `__xljs_exports`
)

// Bundle resolves imports starting at entry and returns a single script
// (with an inline source map) that the engine can run. TypeScript and
// syntax that goja does not understand are transformed by esbuild.
func Bundle(entry string) ([]byte, error) { return bundle(entry, false) }

// bundle is Bundle; withSources puts the original files into the source
// map (for the debugger, which has no other way to show them).
func bundle(entry string, withSources bool) ([]byte, error) {
	abs, err := filepath.Abs(entry)
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(abs); err != nil {
		return nil, err
	}
	sources := api.SourcesContentExclude
	if withSources {
		sources = api.SourcesContentInclude
	}
	res := api.Build(api.BuildOptions{
		EntryPoints:       []string{abs},
		AbsWorkingDir:     filepath.Dir(abs),
		Bundle:            true,
		Write:             false,
		Format:            api.FormatESModule,
		Platform:          api.PlatformNeutral,
		MainFields:        []string{"module", "main"},
		Conditions:        []string{"import", "default"},
		Target:            api.ESNext,
		Supported:         map[string]bool{"async-generator": false},
		Sourcemap:         api.SourceMapInline,
		SourcesContent:    sources,
		Charset:           api.CharsetUTF8,
		LogLevel:          api.LogLevelSilent,
		ResolveExtensions: []string{".ts", ".mts", ".js", ".mjs", ".cjs", ".json"},
		Loader: map[string]api.Loader{
			".txt":  api.LoaderText,
			".json": api.LoaderJSON,
		},
		Banner: map[string]string{"js": bundleBanner},
		Footer: map[string]string{"js": bundleFooter},
	})
	if len(res.Errors) > 0 {
		return nil, &BundleError{Messages: res.Errors}
	}
	if len(res.OutputFiles) == 0 {
		return nil, i18n.Err("engine.emptyBundle")
	}
	return moveExports(res.OutputFiles[0].Contents), nil
}

// moveExports replaces the export clause that esbuild puts at the end of
// the bundle (not valid inside the wrapper function) with a call at the
// start of the wrapper that hands an object of getters to exportsParam.
// The getters make exported functions callable from windows while the
// script is still running (function declarations are hoisted) and keep
// later assignments visible. The call goes on the banner line so that the
// source map stays valid. Code without such a clause is returned as is.
func moveExports(code []byte) []byte {
	s := string(code)
	end := strings.LastIndex(s, "\n"+bundleFooter)
	banner := strings.Index(s, bundleBanner)
	if end < 0 || banner < 0 {
		return code
	}
	body := s[:end+1]
	start := strings.LastIndex(body, "\nexport {\n")
	if start < 0 || !strings.HasSuffix(body, "\n};\n") {
		return code
	}
	clause := body[start+len("\nexport {\n") : len(body)-len("\n};\n")]
	var getters []string
	for _, item := range strings.Split(clause, "\n") { // one per line
		item = strings.TrimSuffix(strings.TrimSpace(item), ",")
		local, name, found := strings.Cut(item, " as ")
		if !found {
			name = local
		}
		if !jsIdent.MatchString(local) || !(jsIdent.MatchString(name) || jsString.MatchString(name)) {
			return code
		}
		getters = append(getters, "get "+name+"() { return "+local+"; }")
	}
	call := exportsParam + "({ " + strings.Join(getters, ", ") + " });"
	at := banner + len(bundleBanner)
	return []byte(s[:at] + call + s[at:start+1] + s[end+1:])
}

var (
	jsIdent  = regexp.MustCompile(`^[\p{L}\p{Nl}$_][\p{L}\p{Nl}\p{Mn}\p{Mc}\p{Nd}\p{Pc}$\x{200c}\x{200d}]*$`)
	jsString = regexp.MustCompile(`^"(?:[^"\\]|\\.)*"$`)
)

// BundleError reports syntax and resolution errors found while bundling.
type BundleError struct {
	Messages []api.Message
}

func (e *BundleError) Error() string {
	var b strings.Builder
	for i, m := range e.Messages {
		if i > 0 {
			b.WriteString("\n")
		}
		if m.Location != nil {
			fmt.Fprintf(&b, "%s:%d:%d: %s", m.Location.File, m.Location.Line, m.Location.Column+1, i18n.Text(m.Text))
			if m.Location.LineText != "" {
				fmt.Fprintf(&b, "\n    %s", m.Location.LineText)
			}
		} else {
			b.WriteString(i18n.Text(m.Text))
		}
	}
	return b.String()
}
