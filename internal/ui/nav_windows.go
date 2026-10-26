//go:build windows

package ui

import (
	"strings"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

// navHandler is a WebView2 event handler (NavigationStarting or
// NewWindowRequested) implemented in Go: it keeps windows on the script's
// pages and opens other links in the default browser. pkg/edge has no
// wrapper for these events.
type navHandler struct {
	vtbl      *navHandlerVtbl
	w         *webWindow
	newWindow bool
}

type navHandlerVtbl struct{ queryInterface, addRef, release, invoke uintptr }

var navVtbl = &navHandlerVtbl{
	queryInterface: syscall.NewCallback(func(this *navHandler, iid uintptr, out **navHandler) uintptr {
		if out != nil {
			*out = this
		}
		return 0
	}),
	// The window owns the handler (webWindow.nav), so there is nothing to count.
	addRef:  syscall.NewCallback(func(this *navHandler) uintptr { return 1 }),
	release: syscall.NewCallback(func(this *navHandler) uintptr { return 1 }),
	invoke:  syscall.NewCallback(navInvoke),
}

// Vtable indices (WebView2.h).
const (
	idxAddNavigationStarting  = 7  // ICoreWebView2::add_NavigationStarting
	idxAddNewWindowRequested  = 44 // ICoreWebView2::add_NewWindowRequested
	idxArgsGetURI             = 3  // get_Uri of both event args
	idxNavigationPutCancel    = 8  // ICoreWebView2NavigationStartingEventArgs::put_Cancel
	idxNewWindowPutHandled    = 6  // ICoreWebView2NewWindowRequestedEventArgs::put_Handled
	navigationStartingHandler = false
	newWindowRequestedHandler = true
)

// keepInside registers the navigation handlers on the window's WebView2.
func (w *webWindow) keepInside() {
	core := (*comObject)(unsafe.Pointer(w.core))
	w.nav = [2]*navHandler{
		{vtbl: navVtbl, w: w, newWindow: navigationStartingHandler},
		{vtbl: navVtbl, w: w, newWindow: newWindowRequestedHandler},
	}
	var token int64
	core.call(idxAddNavigationStarting, uintptr(unsafe.Pointer(w.nav[0])), uintptr(unsafe.Pointer(&token)))
	core.call(idxAddNewWindowRequested, uintptr(unsafe.Pointer(w.nav[1])), uintptr(unsafe.Pointer(&token)))
}

func navInvoke(this *navHandler, sender, args *comObject) uintptr {
	var p *uint16
	if failed(args.call(idxArgsGetURI, uintptr(unsafe.Pointer(&p)))) || p == nil {
		return 0
	}
	uri := windows.UTF16PtrToString(p)
	windows.CoTaskMemFree(unsafe.Pointer(p))
	if this.newWindow {
		args.call(idxNewWindowPutHandled, 1) // no popup window
	} else {
		if inside(uri) {
			return 0
		}
		args.call(idxNavigationPutCancel, 1)
	}
	if external(uri) {
		this.w.n.OpenPath(uri)
	}
	return 0
}

// inside reports whether a window may show uri itself.
func inside(uri string) bool {
	return strings.HasPrefix(uri, AssetOrigin) || strings.HasPrefix(uri, "about:") || strings.HasPrefix(uri, "data:")
}

// external reports whether uri is opened with the default application.
func external(uri string) bool {
	u := strings.ToLower(uri)
	return strings.HasPrefix(u, "https://") || strings.HasPrefix(u, "http://") || strings.HasPrefix(u, "mailto:")
}
