package api

import (
	"embed"
	"path"
	"sort"
	"strings"
)

// Day.js (https://day.js.org, MIT) with the locales in dayjs/locale (one per
// language in internal/i18n/locales besides English, which is built in) and
// the plugins in dayjs/plugin, unmodified from the npm package (version in
// dayjs/README.md). To update, copy dayjs.min.js and the same locale and
// plugin files from a newer package.
//
//go:embed dayjs/dayjs.min.js dayjs/locale/*.js dayjs/plugin/*.js
var dayjsFS embed.FS

// dayjsPlaceholder in prelude.js is replaced by dayjsBundle().
const dayjsPlaceholder = "/*@dayjs@*/null"

// dayjsBundle returns a JavaScript expression that evaluates to
// { dayjs, locales, plugins } without defining any globals: each UMD file
// runs as a CommonJS module (module, exports and require are provided).
func dayjsBundle() string {
	var b strings.Builder
	b.WriteString("(function () { const mods = {}; const require = (name) => mods[name]; ")
	b.WriteString("const load = (name, fn) => { const module = { exports: {} }; fn(module, module.exports, require); mods[name] = module.exports; }; ")
	module := func(name, file string) {
		src, err := dayjsFS.ReadFile(file)
		if err != nil {
			panic(err)
		}
		b.WriteString("load(" + quoteJS(name) + ", function (module, exports, require) {\n")
		b.Write(src)
		b.WriteString("\n});\n")
	}
	module("dayjs", "dayjs/dayjs.min.js")
	// Loading a locale file registers it with dayjs.
	locales := dayjsNames("dayjs/locale")
	for _, l := range locales {
		module("dayjs/locale/"+l, path.Join("dayjs/locale", l+".js"))
	}
	plugins := dayjsNames("dayjs/plugin")
	for _, p := range plugins {
		module("dayjs/plugin/"+p, path.Join("dayjs/plugin", p+".js"))
	}
	b.WriteString("return { dayjs: mods.dayjs, locales: [")
	for i, l := range locales {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(quoteJS(l))
	}
	b.WriteString("], plugins: [")
	for i, p := range plugins {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString("mods[" + quoteJS("dayjs/plugin/"+p) + "]")
	}
	b.WriteString("] }; })()")
	return b.String()
}

// dayjsNames lists the .js files in dir without the extension, sorted.
func dayjsNames(dir string) []string {
	entries, err := dayjsFS.ReadDir(dir)
	if err != nil {
		panic(err)
	}
	var names []string
	for _, e := range entries {
		names = append(names, strings.TrimSuffix(e.Name(), ".js"))
	}
	sort.Strings(names)
	return names
}

func quoteJS(s string) string { return `"` + strings.ReplaceAll(s, `"`, `\"`) + `"` }
