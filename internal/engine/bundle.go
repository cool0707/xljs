package engine

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/evanw/esbuild/pkg/api"
)

// The bundled script is wrapped in an async function so that top-level
// await works. The program evaluates to that function; the engine calls it
// (so no extra wrapper frame appears in stack traces).
const (
	bundleBanner = `(async () => {"use strict";`
	bundleFooter = `})`
)

// Bundle resolves imports starting at entry and returns a single script
// (with an inline source map) that the engine can run. TypeScript and
// syntax that goja does not understand are transformed by esbuild.
func Bundle(entry string) ([]byte, error) {
	abs, err := filepath.Abs(entry)
	if err != nil {
		return nil, err
	}
	if _, err := os.Stat(abs); err != nil {
		return nil, err
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
		SourcesContent:    api.SourcesContentExclude,
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
		return nil, errors.New(i18n.S("バンドル結果が空です", "esbuild produced no output"))
	}
	return res.OutputFiles[0].Contents, nil
}

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
