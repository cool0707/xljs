//! Shell links through IShellLinkW and IPersistFile (Windows).

use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;
use windows::core::{HSTRING, Interface};

use super::Link;
use crate::{Error, Result, err};

/// COM on this thread for the life of the guard (declare it before the COM
/// objects, so that it is dropped after them).
struct Com(bool);

impl Com {
    fn init() -> Com {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        Com(hr.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// A Win32 error HRESULT (0x8007xxxx) becomes a path error; others name the
/// HRESULT's text.
fn com_err(write: bool, path: &str, e: windows::core::Error) -> Error {
    let code = e.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        return Error::path(
            "open",
            path,
            std::io::Error::from_raw_os_error((code & 0xFFFF) as i32),
        );
    }
    let msg = e.message();
    let msg = if msg.trim().is_empty() {
        format!("HRESULT 0x{code:08X}")
    } else {
        msg.trim().to_string()
    };
    if write {
        err!("shell.shortcutWrite", path, msg)
    } else {
        err!("shell.shortcutRead", path, msg)
    }
}

fn wide_str(b: &[u16]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

fn create() -> windows::core::Result<IShellLinkW> {
    unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
}

pub fn load(path: &str) -> Result<Link> {
    let _com = Com::init();
    let read_err = |e| com_err(false, path, e);
    unsafe {
        let sl = create().map_err(read_err)?;
        let pf: IPersistFile = sl.cast().map_err(read_err)?;
        pf.Load(&HSTRING::from(path), STGM_READ)
            .map_err(|_| err!("shell.notShortcut", path))?;
        let mut buf = vec![0u16; 32768];
        let mut l = Link::default();
        // Without SLGP_RAWPATH, environment variables come expanded.
        if sl.GetPath(&mut buf, std::ptr::null_mut(), 0).is_ok() {
            l.target = wide_str(&buf);
        }
        if l.target.is_empty() {
            // A shell item that is not a file (Control Panel, shell:AppsFolder\...).
            if let Ok(pidl) = sl.GetIDList()
                && !pidl.is_null()
            {
                if let Ok(name) = SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING) {
                    l.target = name.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(name.0 as *const _));
                }
                ILFree(Some(pidl));
            }
        }
        buf.fill(0);
        if sl.GetArguments(&mut buf).is_ok() {
            l.args = wide_str(&buf);
        }
        buf.fill(0);
        if sl.GetWorkingDirectory(&mut buf).is_ok() {
            l.cwd = wide_str(&buf);
        }
        buf.fill(0);
        if sl.GetDescription(&mut buf).is_ok() {
            l.description = wide_str(&buf);
        }
        buf.fill(0);
        let mut index = 0;
        if sl.GetIconLocation(&mut buf, &mut index).is_ok() {
            // Windows keeps "C:\Windows\..." as "%SystemRoot%\..."; scripts get
            // the path, as for the target.
            l.icon = super::expand_env(&wide_str(&buf));
            l.icon_index = index;
        }
        l.show = sl
            .GetShowCmd()
            .map(|s| s.0 as u32)
            .unwrap_or(super::SW_SHOWNORMAL);
        l.hotkey = sl.GetHotkey().unwrap_or(0);
        if let Ok(dl) = sl.cast::<IShellLinkDataList>() {
            l.run_as_admin = dl
                .GetFlags()
                .is_ok_and(|f| f & SLDF_RUNAS_USER.0 as u32 != 0);
        }
        Ok(l)
    }
}

/// Saves l at path. With prev (the link as read from path) the file is
/// loaded first and only the fields that differ are set, so that what
/// scripts cannot see (the item ID list, other data blocks) stays.
pub fn save(path: &str, l: &Link, prev: Option<&Link>) -> Result<()> {
    let _com = Com::init();
    let write_err = |e| com_err(true, path, e);
    unsafe {
        let sl = create().map_err(write_err)?;
        let pf: IPersistFile = sl.cast().map_err(write_err)?;
        if prev.is_some() {
            pf.Load(&HSTRING::from(path), STGM_READWRITE)
                .map_err(write_err)?;
        }
        let changed = |f: fn(&Link) -> &str| prev.is_none_or(|p| f(p) != f(l));
        if changed(|l| &l.target) {
            sl.SetPath(&HSTRING::from(&l.target)).map_err(write_err)?;
        }
        if changed(|l| &l.args) {
            sl.SetArguments(&HSTRING::from(&l.args))
                .map_err(write_err)?;
        }
        if changed(|l| &l.cwd) {
            sl.SetWorkingDirectory(&HSTRING::from(&l.cwd))
                .map_err(write_err)?;
        }
        if changed(|l| &l.description) {
            sl.SetDescription(&HSTRING::from(&l.description))
                .map_err(write_err)?;
        }
        if changed(|l| &l.icon) || prev.is_some_and(|p| p.icon_index != l.icon_index) {
            sl.SetIconLocation(&HSTRING::from(&l.icon), l.icon_index)
                .map_err(write_err)?;
        }
        sl.SetShowCmd(SHOW_WINDOW_CMD(l.show as i32))
            .map_err(write_err)?;
        sl.SetHotkey(l.hotkey).map_err(write_err)?;
        let dl: IShellLinkDataList = sl.cast().map_err(write_err)?;
        let flags = dl.GetFlags().map_err(write_err)?;
        let bit = SLDF_RUNAS_USER.0 as u32;
        let want = if l.run_as_admin {
            flags | bit
        } else {
            flags & !bit
        };
        if want != flags {
            dl.SetFlags(want).map_err(write_err)?;
        }
        pf.Save(&HSTRING::from(path), true).map_err(write_err)?;
        Ok(())
    }
}
