package i18n

import (
	"errors"
	"io/fs"
	"os"
	"runtime"
	"strings"
	"syscall"
)

// Text translates well-known English library messages in s with the
// active locale's translation rules.
func Text(s string) string {
	loc := Current()
	if s == "" || len(loc.rules) == 0 {
		return s
	}
	for i, re := range loc.rules {
		s = re.ReplaceAllString(s, loc.Translations[i][1])
	}
	return s
}

// reason describes the OS-level cause of err.
func reason(err error) string {
	var errno syscall.Errno
	if runtime.GOOS == "windows" && errors.As(err, &errno) {
		switch errno {
		case 32, 33: // ERROR_SHARING_VIOLATION, ERROR_LOCK_VIOLATION
			return T("os.reason.inUse")
		}
		// Other Windows error texts come from the system in its language.
		return Text(err.Error())
	}
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return T("os.reason.notExist")
	case errors.Is(err, fs.ErrPermission):
		return T("os.reason.permission")
	case errors.Is(err, fs.ErrExist):
		return T("os.reason.exist")
	}
	if errors.As(err, &errno) {
		switch errno {
		case syscall.ENOTDIR:
			return T("os.reason.notDir")
		case syscall.EISDIR:
			return T("os.reason.isDir")
		case syscall.ENOTEMPTY:
			return T("os.reason.notEmpty")
		case syscall.ENOSPC:
			return T("os.reason.noSpace")
		}
	}
	return Text(err.Error())
}

func opName(op string) string {
	switch op {
	case "unlinkat":
		op = "remove"
	case "lstat":
		op = "stat"
	case "open dir":
		op = "readdir"
	}
	key := "os.op." + op
	if m := Message(key); m != key {
		return m
	}
	return op
}

// Error returns the user-facing message for err in the active language.
func Error(err error) string {
	if err == nil {
		return ""
	}
	var le *os.LinkError
	if errors.As(err, &le) {
		return T("os.error", le.Old+" → "+le.New, opName(le.Op), reason(le.Err))
	}
	var pe *fs.PathError
	if errors.As(err, &pe) {
		msg := T("os.error", pe.Path, opName(pe.Op), reason(pe.Err))
		// Keep any context added around the path error.
		if full := err.Error(); full != pe.Error() {
			if i := strings.Index(full, pe.Error()); i > 0 {
				msg = Text(full[:i]) + msg
			}
		}
		return msg
	}
	return Text(err.Error())
}
