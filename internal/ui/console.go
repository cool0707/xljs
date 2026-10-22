package ui

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/mattn/go-runewidth"
	"golang.org/x/term"
)

// Console implements UI on top of stdin/stderr.
type Console struct {
	In  io.Reader
	Out io.Writer // usually os.Stderr

	once   sync.Once
	reader *bufio.Reader
}

// NewConsole returns a console UI bound to the process stdin/stderr.
func NewConsole() *Console { return &Console{In: os.Stdin, Out: os.Stderr} }

func (c *Console) Name() string { return "console" }

func (c *Console) readLine() (string, bool) {
	c.once.Do(func() { c.reader = bufio.NewReader(c.In) })
	line, err := c.reader.ReadString('\n')
	if err != nil && line == "" {
		return "", false
	}
	return strings.TrimRight(line, "\r\n"), true
}

func (c *Console) interactive() bool {
	f, ok := c.In.(*os.File)
	return ok && term.IsTerminal(int(f.Fd()))
}

func (c *Console) header(title string) string {
	if title == "" {
		return ""
	}
	return "[" + title + "] "
}

func (c *Console) Alert(title, message string) {
	fmt.Fprintf(c.Out, "%s%s\n", c.header(title), message)
	if c.interactive() {
		fmt.Fprint(c.Out, i18n.T("ui.pressEnter"))
		c.readLine()
	}
}

func (c *Console) Confirm(title, message string) bool {
	fmt.Fprintf(c.Out, "%s%s [y/N]: ", c.header(title), message)
	line, ok := c.readLine()
	if !ok {
		fmt.Fprintln(c.Out)
		return false
	}
	switch strings.ToLower(strings.TrimSpace(line)) {
	case "y", "yes", "はい", "ok":
		return true
	}
	return false
}

func (c *Console) Prompt(title, message, def string) (string, bool) {
	if def != "" {
		fmt.Fprintf(c.Out, "%s%s [%s]: ", c.header(title), message, def)
	} else {
		fmt.Fprintf(c.Out, "%s%s: ", c.header(title), message)
	}
	line, ok := c.readLine()
	if !ok {
		fmt.Fprintln(c.Out)
		return "", false
	}
	if line == "" {
		return def, true
	}
	return line, true
}

func (c *Console) askPath(title, message string) []string {
	line, ok := c.Prompt(title, message, "")
	if !ok || strings.TrimSpace(line) == "" {
		return nil
	}
	var out []string
	for _, p := range strings.Split(line, ";") {
		p = strings.Trim(strings.TrimSpace(p), `"`)
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func filterHint(fs []FileFilter) string {
	var pats []string
	for _, f := range fs {
		pats = append(pats, f.Patterns...)
	}
	if len(pats) == 0 {
		return ""
	}
	return " (" + strings.Join(pats, ", ") + ")"
}

func (c *Console) OpenFiles(o PickerOptions) []string {
	msg := i18n.T("ui.fileToOpen") + filterHint(o.Filters)
	if o.Multiple {
		msg += i18n.T("ui.multipleHint")
	}
	paths := c.askPath(o.Title, msg)
	if !o.Multiple && len(paths) > 1 {
		paths = paths[:1]
	}
	return paths
}

func (c *Console) SaveFile(o PickerOptions) (string, bool) {
	def := o.SuggestedName
	if def != "" && o.StartIn != "" {
		def = filepath.Join(o.StartIn, def)
	}
	p, ok := c.Prompt(o.Title, i18n.T("ui.saveAs")+filterHint(o.Filters), def)
	p = strings.Trim(strings.TrimSpace(p), `"`)
	return p, ok && p != ""
}

func (c *Console) PickDirectory(o PickerOptions) (string, bool) {
	p := c.askPath(o.Title, i18n.T("ui.folder"))
	if len(p) == 0 {
		return "", false
	}
	return p[0], true
}

func (c *Console) ShowError(title, message string) {
	fmt.Fprintf(c.Out, "%s\n", message)
}

func (c *Console) Progress(o ProgressOptions) Progress {
	p := &consoleProgress{out: c.Out, title: o.Title, total: o.Total, message: o.Message}
	if f, ok := c.Out.(*os.File); ok && term.IsTerminal(int(f.Fd())) {
		p.tty = true
		if w, _, err := term.GetSize(int(f.Fd())); err == nil && w > 20 {
			p.width = w
		}
	}
	if p.width == 0 {
		p.width = 80
	}
	activeProgress.Add(1)
	p.render(true)
	return p
}

type consoleProgress struct {
	mu       sync.Mutex
	out      io.Writer
	tty      bool
	width    int
	title    string
	done     int
	total    int
	message  string
	last     time.Time
	lastPct  int
	closed   bool
	spinner  int
	rendered bool
	lastLine string
}

func (p *consoleProgress) Update(done, total int, message string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.closed {
		return
	}
	p.done = done
	if total > 0 {
		p.total = total
	}
	p.message = message
	p.render(false)
}

func (p *consoleProgress) Cancelled() bool { return Interrupted() }

func (p *consoleProgress) Close() {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.closed {
		return
	}
	p.render(true)
	if p.tty && p.rendered {
		fmt.Fprintln(p.out)
	}
	p.closed = true
	activeProgress.Add(-1)
}

func (p *consoleProgress) render(force bool) {
	now := time.Now()
	if p.tty {
		if !force && now.Sub(p.last) < 100*time.Millisecond {
			return
		}
		p.last = now
		fmt.Fprint(p.out, "\r"+p.line()+"\x1b[K")
		p.rendered = true
		return
	}
	// Not a terminal: print a line every 10% (or at start/end).
	pct := -1
	if p.total > 0 {
		pct = p.done * 100 / p.total
	}
	if force || (pct >= 0 && pct/10 != p.lastPct/10) {
		p.lastPct = pct
		line := p.line()
		if line == p.lastLine {
			return
		}
		p.lastLine = line
		fmt.Fprintln(p.out, line)
		p.rendered = true
	}
}

func (p *consoleProgress) line() string {
	var b strings.Builder
	if p.title != "" {
		b.WriteString(p.title)
		b.WriteString(" ")
	}
	if p.total > 0 {
		const barW = 20
		filled := p.done * barW / p.total
		if filled > barW {
			filled = barW
		}
		if filled < 0 {
			filled = 0
		}
		fmt.Fprintf(&b, "[%s%s] %d/%d (%d%%)", strings.Repeat("#", filled), strings.Repeat("-", barW-filled),
			p.done, p.total, p.done*100/p.total)
	} else {
		frames := `|/-\`
		p.spinner = (p.spinner + 1) % len(frames)
		fmt.Fprintf(&b, "[%c] %d", frames[p.spinner], p.done)
	}
	if p.message != "" {
		b.WriteString(" ")
		b.WriteString(p.message)
	}
	s := b.String()
	if p.tty {
		s = runewidth.Truncate(s, p.width-1, "…")
	}
	return s
}

func (c *Console) ShowInFolder(paths []string) error { return ShellShowInFolder(paths) }
func (c *Console) OpenPath(target string) error      { return ShellOpen(target) }
