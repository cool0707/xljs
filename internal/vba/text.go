package vba

import (
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
)

// Text formats the project of the workbook named file as Markdown for
// pasting into an AI chat: a heading per module and its code in a vba code
// block. The Attribute lines at the top of the modules are left out, and so
// are modules without code (only Option statements), which are listed.
// Without a file name the project's name is used.
func Text(file string, p *Project) string {
	if file == "" {
		file = p.Name
	}
	var b strings.Builder
	b.WriteString(i18n.T("vba.textTitle", file) + "\n\n")
	b.WriteString(i18n.T("vba.textIntro", p.Name, file, len(p.Modules)) + "\n")
	var empty []string
	for _, m := range p.Modules {
		code := body(m.Code)
		if code == "" {
			empty = append(empty, m.Name)
			continue
		}
		fence := strings.Repeat("`", max(3, longestRun(code, '`')+1))
		b.WriteString("\n" + i18n.T("vba.textModule", m.Name, describe(m)) + "\n\n")
		b.WriteString(fence + "vba\n" + code + "\n" + fence + "\n")
	}
	if len(empty) > 0 {
		b.WriteString("\n" + i18n.T("vba.textEmpty", strings.Join(empty, i18n.T("vba.separator"))) + "\n")
	}
	return b.String()
}

func describe(m Module) string {
	switch m.Type {
	case Standard:
		return i18n.T("vba.type.standard")
	case Class:
		return i18n.T("vba.type.class")
	case Form:
		return i18n.T("vba.type.form")
	}
	switch {
	case m.Sheet != "":
		return i18n.T("vba.type.sheet", m.Sheet)
	case strings.EqualFold(m.Name, "ThisWorkbook"):
		return i18n.T("vba.type.workbook")
	}
	return i18n.T("vba.type.document")
}

// body is the code without the leading Attribute lines and the blank lines
// around it, or "" when nothing but Option statements remain.
func body(code string) string {
	lines := strings.Split(code, "\n")
	for len(lines) > 0 && strings.HasPrefix(lines[0], "Attribute VB_") {
		lines = lines[1:]
	}
	s := strings.Trim(strings.Join(lines, "\n"), " \t\n")
	for _, l := range strings.Split(s, "\n") {
		l = strings.TrimSpace(l)
		if l != "" && !strings.HasPrefix(strings.ToLower(l), "option ") {
			return s
		}
	}
	return ""
}

func longestRun(s string, c byte) int {
	best, n := 0, 0
	for i := 0; i < len(s); i++ {
		if s[i] == c {
			n++
			best = max(best, n)
		} else {
			n = 0
		}
	}
	return best
}
