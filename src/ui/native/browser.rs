//! The browser pane of a window (WindowOptions::browser): a second WebView2
//! in a child window over a part of the page, showing web sites (the AI
//! chats of the development mode). The page tells where it goes
//! (Window::browser); downloads are saved to the pane's folder without
//! asking and reported when they are done.

use std::cell::Cell;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Interface, PWSTR, w};

use super::hstr;
use crate::ui::{BrowserCommand, BrowserEvent, BrowserOptions};

/// The pane of a window.
pub(super) struct Browser {
    host: HWND,
    controller: ICoreWebView2Controller,
    core: ICoreWebView2,
    visible: Cell<bool>,
}

static CLASS: OnceLock<bool> = OnceLock::new();

fn register_class() {
    CLASS.get_or_init(|| unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(host_proc),
            hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("xljsBrowserHost"),
            ..Default::default()
        };
        RegisterClassExW(&wc) != 0
    });
}

unsafe extern "system" fn host_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn take_pwstr(p: PWSTR) -> String {
    super::webview::take_pwstr(p)
}

/// A path in dir for a download named name that is not taken yet.
fn free_path(dir: &Path, name: &str) -> std::path::PathBuf {
    let name = if name.is_empty() { "download" } else { name };
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".into());
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut path = dir.join(name);
    let mut i = 1;
    while path.exists() {
        path = dir.join(format!("{stem} ({i}){ext}"));
        i += 1;
    }
    path
}

/// Creates the pane (hidden) in the window parent with the environment of
/// its page.
pub(super) fn create(
    parent: HWND,
    env: &ICoreWebView2Environment,
    o: &BrowserOptions,
) -> std::result::Result<Browser, String> {
    register_class();
    unsafe {
        let host = CreateWindowExW(
            Default::default(),
            w!("xljsBrowserHost"),
            w!(""),
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            100,
            100,
            Some(parent),
            None,
            Some(GetModuleHandleW(None).unwrap_or_default().into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
            move |hr: windows::core::Result<()>, c| {
                let _ = tx.send(hr.map(|_| c));
                Ok(())
            },
        ));
        let made = (|| -> std::result::Result<(ICoreWebView2Controller, ICoreWebView2), String> {
            env.CreateCoreWebView2Controller(host, &handler)
                .map_err(|e| e.to_string())?;
            let controller = wait_with_pump(rx)
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no controller".to_string())?;
            let core = controller.CoreWebView2().map_err(|e| e.to_string())?;
            Ok((controller, core))
        })();
        let (controller, core) = match made {
            Ok(x) => x,
            Err(e) => {
                let _ = DestroyWindow(host);
                return Err(e);
            }
        };
        if let Ok(s) = core.Settings() {
            let _ = s.SetAreDevToolsEnabled(false);
            let _ = s.SetIsStatusBarEnabled(false);
            // The User-Agent of the Edge or Chrome installed, with its
            // client hints (WebView2's own name WebView2 among the brands),
            // before the first navigation. Popups keep WebView2's.
            if let Some(a) = &o.agent {
                if let Ok(s2) = s.cast::<ICoreWebView2Settings2>() {
                    let _ = s2.SetUserAgent(&hstr(&a.user_agent));
                }
                if let Some(p) = &a.devtools {
                    super::webview::devtools(&core, "Emulation.setUserAgentOverride", p, |_, _| {});
                }
            }
        }
        let _ = controller.SetIsVisible(false);
        let mut token = 0i64;
        super::auth::attach(&core);

        // The page shown: the session keeps it for the script.
        let on = o.on_event.clone();
        let _ = core.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(
                move |sender: Option<ICoreWebView2>, _| {
                    if let Some(c) = sender {
                        let mut p = PWSTR::null();
                        if c.Source(&mut p).is_ok() {
                            on(BrowserEvent::Url(take_pwstr(p)));
                        }
                    }
                    Ok(())
                },
            )),
            &mut token,
        );

        // Text files (the answers of the chat) are saved without the check
        // for dangerous file types, which asks before keeping every .js
        // file (WebView2 1.0.2903+; older runtimes still ask).
        if let Ok(core26) = core.cast::<ICoreWebView2_26>() {
            let trusted = o.trusted_extensions.clone();
            let _ = core26.add_SaveFileSecurityCheckStarting(
                &SaveFileSecurityCheckStartingEventHandler::create(Box::new(
                    move |_, args: Option<ICoreWebView2SaveFileSecurityCheckStartingEventArgs>| {
                        let Some(args) = args else { return Ok(()) };
                        let mut p = PWSTR::null();
                        if args.FileExtension(&mut p).is_ok() {
                            let ext = take_pwstr(p).trim_start_matches('.').to_lowercase();
                            if trusted.iter().any(|t| *t == ext) {
                                let _ = args.SetSuppressDefaultPolicy(true);
                            }
                        }
                        Ok(())
                    },
                )),
                &mut token,
            );
        }

        // Downloads go to the pane's folder without the download bubble.
        if let Ok(core4) = core.cast::<ICoreWebView2_4>() {
            let (on, dir) = (o.on_event.clone(), o.downloads.clone());
            let _ = core4.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(
                    move |_, args: Option<ICoreWebView2DownloadStartingEventArgs>| {
                        let Some(args) = args else { return Ok(()) };
                        let mut p = PWSTR::null();
                        let _ = args.ResultFilePath(&mut p);
                        let suggested = take_pwstr(p);
                        let name = Path::new(&suggested)
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let _ = std::fs::create_dir_all(&dir);
                        let path = free_path(Path::new(&dir), &name);
                        let _ = args.SetResultFilePath(&hstr(&path.to_string_lossy()));
                        let _ = args.SetHandled(true);
                        let Ok(op) = args.DownloadOperation() else {
                            return Ok(());
                        };
                        let on = on.clone();
                        let path = path.to_string_lossy().into_owned();
                        let mut t = 0i64;
                        let _ = op.add_StateChanged(
                            &StateChangedEventHandler::create(Box::new(
                                move |sender: Option<ICoreWebView2DownloadOperation>, _| {
                                    let Some(op) = sender else { return Ok(()) };
                                    let mut st = COREWEBVIEW2_DOWNLOAD_STATE::default();
                                    let _ = op.State(&mut st);
                                    if st == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
                                        on(BrowserEvent::Downloaded {
                                            path: path.clone(),
                                            name: name.clone(),
                                        });
                                    } else if st == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                                        on(BrowserEvent::DownloadFailed { name: name.clone() });
                                    }
                                    Ok(())
                                },
                            )),
                            &mut t,
                        );
                        Ok(())
                    },
                )),
                &mut token,
            );
        }
        Ok(Browser {
            host,
            controller,
            core,
            visible: Cell::new(false),
        })
    }
}

impl Browser {
    pub(super) fn command(&self, cmd: BrowserCommand) {
        unsafe {
            match cmd {
                BrowserCommand::Show {
                    x,
                    y,
                    width,
                    height,
                    scale,
                } => {
                    let px = |v: f64| (v * scale).round() as i32;
                    let (w, h) = (px(width).max(0), px(height).max(0));
                    let _ = SetWindowPos(
                        self.host,
                        Some(HWND_TOP),
                        px(x),
                        px(y),
                        w,
                        h,
                        SWP_SHOWWINDOW | SWP_NOACTIVATE,
                    );
                    let _ = self.controller.SetBounds(RECT {
                        left: 0,
                        top: 0,
                        right: w,
                        bottom: h,
                    });
                    if !self.visible.replace(true) {
                        let _ = self.controller.SetIsVisible(true);
                    }
                }
                BrowserCommand::Hide => {
                    if self.visible.replace(false) {
                        let _ = self.controller.SetIsVisible(false);
                    }
                    let _ = ShowWindow(self.host, SW_HIDE);
                }
                BrowserCommand::Navigate(url) => {
                    let _ = self.core.Navigate(&hstr(&url));
                }
                BrowserCommand::Back => {
                    let _ = self.core.GoBack();
                }
                BrowserCommand::Forward => {
                    let _ = self.core.GoForward();
                }
                BrowserCommand::Reload => {
                    let _ = self.core.Reload();
                }
                BrowserCommand::DragFiles(files) => drag_files(self.host, &files),
            }
        }
    }

    pub(super) fn moved(&self) {
        unsafe {
            let _ = self.controller.NotifyParentWindowPositionChanged();
        }
    }

    pub(super) fn close(&self) {
        unsafe {
            let _ = self.controller.Close();
            let _ = DestroyWindow(self.host);
        }
    }
}

/// Drags files with the shell's data object (what Explorer gives a drop:
/// CF_HDROP and the item list) and its drop source; the modal loop ends
/// when the button is released or Escape is pressed.
fn drag_files(hwnd: HWND, files: &[String]) {
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, OleInitialize};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        BHID_DataObject, ILFree, SHCreateShellItemArrayFromIDLists, SHDoDragDrop,
        SHParseDisplayName,
    };
    unsafe {
        // The page asked while the button was down; it may be up by now.
        let button = if GetSystemMetrics(SM_SWAPBUTTON) != 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        if (GetAsyncKeyState(button.0 as i32) as u16 & 0x8000) == 0 {
            return;
        }
        // Drag and drop needs OLE on this thread (COM is there already);
        // it stays initialized.
        thread_local!(static OLE: bool = unsafe { OleInitialize(None).is_ok() });
        if !OLE.with(|ok| *ok) {
            return;
        }
        let mut pidls: Vec<*mut ITEMIDLIST> = Vec::new();
        for f in files {
            let mut p = std::ptr::null_mut();
            if SHParseDisplayName(&hstr(f), None, &mut p, 0, None).is_ok() && !p.is_null() {
                pidls.push(p);
            }
        }
        if !pidls.is_empty() {
            let list: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| *p as *const _).collect();
            let data = SHCreateShellItemArrayFromIDLists(&list)
                .and_then(|a| a.BindToHandler::<_, IDataObject>(None, &BHID_DataObject));
            if let Ok(data) = data {
                let _ = SHDoDragDrop(Some(hwnd), &data, None, DROPEFFECT_COPY | DROPEFFECT_LINK);
            }
        }
        for p in pidls {
            ILFree(Some(p));
        }
    }
}
