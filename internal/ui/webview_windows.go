//go:build windows

package ui

import (
	_ "embed"
	"io"
	"io/fs"
	"log"
	"mime"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"unsafe"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/wailsapp/go-webview2/pkg/edge"
	"github.com/wailsapp/go-webview2/webviewloader"
	"golang.org/x/sys/windows"
)

//go:embed bridge.js
var bridgeJS string

// WebView2DownloadURL is where users get the WebView2 runtime.
const WebView2DownloadURL = "https://developer.microsoft.com/microsoft-edge/webview2/"

var (
	procShowWindow           = moduser32.NewProc("ShowWindow")
	procUpdateWindow         = moduser32.NewProc("UpdateWindow")
	procLoadCursorW          = moduser32.NewProc("LoadCursorW")
	procGetDpiForSystem      = moduser32.NewProc("GetDpiForSystem")
	procAdjustWindowRectEx   = moduser32.NewProc("AdjustWindowRectEx")
	procSystemParametersInfo = moduser32.NewProc("SystemParametersInfoW")
	procPostQuitMessage      = moduser32.NewProc("PostQuitMessage")
	procExtractIconExW       = modshell32.NewProc("ExtractIconExW")
)

const (
	wsOverlappedWindow = 0x00CF0000
	swShow             = 5
	idcArrow           = 32512
	spiGetWorkArea     = 0x0030
	wmDestroy          = 0x0002
	wmMove             = 0x0003
	wmSize             = 0x0005
	wmActivate         = 0x0006
	wmSetIcon          = 0x0080
	iconSmall          = 0
	iconBig            = 1

	defaultWidth  = 960
	defaultHeight = 640
)

type rect struct{ left, top, right, bottom int32 }

// webWindow is a top-level window hosting WebView2.
type webWindow struct {
	n      *Native
	hwnd   uintptr
	wv     *edge.Chromium
	core   *edge.ICoreWebView2
	o      WindowOptions
	ready  bool
	closed bool
}

var (
	webClassOnce sync.Once
	webClass     *uint16
	webIcons     [2]uintptr // big, small
	webWindows   = map[uintptr]*webWindow{}
)

func registerWebClass() {
	webClassOnce.Do(func() {
		// The WebView2 wrapper logs every environment creation; xljs has no
		// other use for the standard logger.
		log.SetOutput(io.Discard)
		webClass = wstr("xljsWindow")
		if exe, err := os.Executable(); err == nil {
			p := wstr(exe)
			procExtractIconExW.Call(uintptr(unsafe.Pointer(p)), 0,
				uintptr(unsafe.Pointer(&webIcons[0])), uintptr(unsafe.Pointer(&webIcons[1])), 1)
		}
		hinst, _, _ := procGetModuleHandleW.Call(0)
		cursor, _, _ := procLoadCursorW.Call(0, idcArrow)
		wc := wndClassEx{
			wndProc:   syscall.NewCallback(webWndProc),
			instance:  hinst,
			icon:      webIcons[0],
			iconSm:    webIcons[1],
			cursor:    cursor,
			className: webClass,
		}
		wc.size = uint32(unsafe.Sizeof(wc))
		procRegisterClassExW.Call(uintptr(unsafe.Pointer(&wc)))
	})
}

// WebView2Version returns the installed WebView2 runtime version, or "".
func WebView2Version() string {
	v, err := webviewloader.GetAvailableCoreWebView2BrowserVersionString("")
	if err != nil {
		return ""
	}
	return v
}

// OpenWindow opens a window showing o.URL in WebView2.
func (n *Native) OpenWindow(o WindowOptions) (Window, error) {
	if WebView2Version() == "" {
		return nil, i18n.Err("ui.webview2Missing", WebView2DownloadURL)
	}
	registerWebClass()
	w := &webWindow{n: n, o: o}

	width, height := o.Width, o.Height
	if width <= 0 {
		width = defaultWidth
	}
	if height <= 0 {
		height = defaultHeight
	}
	dpi, _, _ := procGetDpiForSystem.Call()
	if dpi == 0 {
		dpi = 96
	}
	r := rect{0, 0, int32(uintptr(width) * dpi / 96), int32(uintptr(height) * dpi / 96)}
	procAdjustWindowRectEx.Call(uintptr(unsafe.Pointer(&r)), wsOverlappedWindow, 0, 0)
	ww, wh := r.right-r.left, r.bottom-r.top
	var work rect
	procSystemParametersInfo.Call(spiGetWorkArea, 0, uintptr(unsafe.Pointer(&work)), 0)
	x := work.left + max(0, (work.right-work.left-ww)/2)
	y := work.top + max(0, (work.bottom-work.top-wh)/2)

	title := wstr(o.Title)
	hinst, _, _ := procGetModuleHandleW.Call(0)
	hwnd, _, err := procCreateWindowExW.Call(0, uintptr(unsafe.Pointer(webClass)), uintptr(unsafe.Pointer(title)),
		wsOverlappedWindow, uintptr(x), uintptr(y), uintptr(ww), uintptr(wh), 0, 0, hinst, 0)
	if hwnd == 0 {
		return nil, err
	}
	w.hwnd = hwnd
	webWindows[hwnd] = w
	if webIcons[0] != 0 {
		procSendMessageW.Call(hwnd, wmSetIcon, iconBig, webIcons[0])
		procSendMessageW.Call(hwnd, wmSetIcon, iconSmall, webIcons[1])
	}
	procShowWindow.Call(hwnd, swShow)
	procUpdateWindow.Call(hwnd)
	procSetForegroundWindow.Call(hwnd)

	if err := w.embed(); err != nil {
		w.o.OnClosed = nil // the caller learns from the error, not from OnClosed
		procDestroyWindow.Call(hwnd)
		return nil, err
	}
	n.wins = append(n.wins, w)
	return w, nil
}

// embed creates the WebView2 control. The wrapper reports errors through
// its error callback and then usually ends the process, so the callback
// shows them. When the environment cannot be created it neither exits nor
// returns: Embed waits in a message loop that only WM_QUIT ends, after
// which it dereferences the missing webview (recovered below).
func (w *webWindow) embed() (err error) {
	wv := edge.NewChromium()
	wv.Debug = w.o.Debug
	if dir, e := os.UserCacheDir(); e == nil {
		wv.DataPath = filepath.Join(dir, "xljs", "WebView2")
	}
	wv.SetErrorCallback(func(e error) {
		w.n.ShowError(w.o.Title, i18n.T("ui.webviewError", i18n.Error(e)))
		if !w.ready {
			procPostQuitMessage.Call(0)
		}
	})
	wv.MessageCallback = func(msg string, _ *edge.ICoreWebView2, _ *edge.ICoreWebView2WebMessageReceivedEventArgs) {
		if w.o.OnMessage != nil {
			w.o.OnMessage(msg, nil)
		}
	}
	wv.MessageWithAdditionalObjectsCallback = func(msg string, _ *edge.ICoreWebView2, args *edge.ICoreWebView2WebMessageReceivedEventArgs) {
		if w.o.OnMessage != nil {
			w.o.OnMessage(msg, messageFiles(args))
		}
	}
	wv.WebResourceRequestedCallback = w.serve
	defer func() {
		if r := recover(); r != nil {
			err = i18n.Err("ui.webviewError", r)
		}
	}()
	if !wv.Embed(w.hwnd) {
		return i18n.Err("ui.webviewError", "Embed")
	}
	w.wv = wv
	if w.core, err = wv.GetController().GetCoreWebView2(); err != nil {
		return i18n.Err("ui.webviewError", err)
	}
	w.ready = true
	if s, e := wv.GetSettings(); e == nil {
		s.PutAreDevToolsEnabled(w.o.Debug)
		s.PutAreDefaultContextMenusEnabled(w.o.Debug)
		s.PutIsStatusBarEnabled(false)
	}
	wv.AddWebResourceRequestedFilter(AssetOrigin+"*", edge.COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
	wv.Init(bridgeJS)
	wv.Resize()
	wv.Focus()
	if w.o.URL != "" {
		wv.Navigate(w.o.URL)
	} else {
		wv.NavigateToString("<!doctype html><title></title>")
	}
	return nil
}

var iidICoreWebView2File = mustGUID("{F2C19559-6BC1-4583-A757-90021BE9AFEC}")

// messageFiles returns the paths of the File objects the page attached with
// postMessageWithAdditionalObjects.
func messageFiles(args *edge.ICoreWebView2WebMessageReceivedEventArgs) []string {
	coll, err := args.GetAdditionalObjects()
	if err != nil || coll == nil {
		return nil
	}
	defer coll.Release()
	count, _ := coll.GetCount()
	var paths []string
	for i := uint32(0); i < count; i++ {
		v, err := coll.GetValueAtIndex(i)
		if err != nil || v == nil {
			continue
		}
		unk := (*comObject)(unsafe.Pointer(v))
		var file *comObject
		if hr := unk.call(0, uintptr(unsafe.Pointer(&iidICoreWebView2File)), uintptr(unsafe.Pointer(&file))); !failed(hr) && file != nil {
			var p *uint16
			if hr := file.call(3, uintptr(unsafe.Pointer(&p))); !failed(hr) && p != nil {
				paths = append(paths, windows.UTF16PtrToString(p))
				windows.CoTaskMemFree(unsafe.Pointer(p))
			}
			file.release()
		}
		unk.release()
	}
	return paths
}

// serve answers requests for AssetOrigin from o.Assets.
func (w *webWindow) serve(req *edge.ICoreWebView2WebResourceRequest, args *edge.ICoreWebView2WebResourceRequestedEventArgs) {
	uri, err := req.GetUri()
	if err != nil || !strings.HasPrefix(uri, AssetOrigin) {
		return
	}
	status, reason, ctype := 200, "OK", "text/plain; charset=utf-8"
	var body []byte
	name := strings.TrimPrefix(uri, AssetOrigin)
	if i := strings.IndexAny(name, "?#"); i >= 0 {
		name = name[:i]
	}
	if u, err := url.PathUnescape(name); err == nil {
		name = u
	}
	if name == "" || strings.HasSuffix(name, "/") {
		name += "index.html"
	}
	if w.o.Assets != nil && fs.ValidPath(name) {
		body, err = fs.ReadFile(w.o.Assets, name)
	} else {
		err = fs.ErrNotExist
	}
	if err != nil {
		status, reason, body = 404, "Not Found", []byte("404 Not Found: "+name)
	} else {
		ctype = contentType(name)
	}
	env := w.wv.Environment()
	resp, err := env.CreateWebResourceResponse(nil, status, reason,
		"Content-Type: "+ctype+"\r\nCache-Control: no-cache")
	if err != nil {
		return
	}
	defer resp.Release()
	if len(body) > 0 {
		resp.PutByteContent(body)
	}
	args.PutResponse(resp)
}

var contentTypes = map[string]string{
	".html":  "text/html; charset=utf-8",
	".htm":   "text/html; charset=utf-8",
	".js":    "text/javascript; charset=utf-8",
	".mjs":   "text/javascript; charset=utf-8",
	".css":   "text/css; charset=utf-8",
	".json":  "application/json; charset=utf-8",
	".svg":   "image/svg+xml",
	".png":   "image/png",
	".jpg":   "image/jpeg",
	".jpeg":  "image/jpeg",
	".gif":   "image/gif",
	".webp":  "image/webp",
	".ico":   "image/x-icon",
	".woff":  "font/woff",
	".woff2": "font/woff2",
	".ttf":   "font/ttf",
	".wasm":  "application/wasm",
	".txt":   "text/plain; charset=utf-8",
	".csv":   "text/csv; charset=utf-8",
}

func contentType(name string) string {
	ext := strings.ToLower(path.Ext(name))
	if t, ok := contentTypes[ext]; ok {
		return t
	}
	if t := mime.TypeByExtension(ext); t != "" {
		return t
	}
	return "application/octet-stream"
}

func webWndProc(hwnd uintptr, msg uint32, wParam, lParam uintptr) uintptr {
	if w := webWindows[hwnd]; w != nil {
		switch msg {
		case wmSize:
			if w.ready {
				w.wv.Resize()
			}
		case wmMove:
			if w.ready {
				w.wv.NotifyParentWindowPositionChanged()
			}
		case wmActivate:
			if w.ready && wParam&0xFFFF != 0 {
				w.wv.Focus()
			}
		case wmDestroy:
			w.destroyed()
		}
	}
	r, _, _ := procDefWindowProcW.Call(hwnd, uintptr(msg), wParam, lParam)
	return r
}

func (w *webWindow) destroyed() {
	if w.closed {
		return
	}
	w.closed = true
	w.ready = false
	if w.wv != nil {
		w.wv.ShuttingDown()
	}
	delete(webWindows, w.hwnd)
	for i, x := range w.n.wins {
		if x == w {
			w.n.wins = append(w.n.wins[:i], w.n.wins[i+1:]...)
			break
		}
	}
	if w.o.OnClosed != nil {
		w.o.OnClosed()
	}
}

// hostPrefix marks messages sent by the host. The WebView2 wrapper echoes
// every page message back to the page, so the page must ignore messages
// without it.
const hostPrefix = "\x02"

func (w *webWindow) Post(msg string) {
	if w.ready {
		w.core.PostWebMessageAsString(hostPrefix + msg)
	}
}

func (w *webWindow) Close() {
	if !w.closed {
		procDestroyWindow.Call(w.hwnd)
	}
}
