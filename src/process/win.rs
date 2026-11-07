//! Starting processes on Windows with CreateProcessW: std::process cannot
//! set the window state (STARTUPINFO.wShowWindow). The standard handles
//! are passed with PROC_THREAD_ATTRIBUTE_HANDLE_LIST so that no other
//! handle of xljs leaks into the child, and the child goes into a job
//! object so that kill() also ends the programs it started (cmd.exe /c).

use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS,
    SetHandleInformation, WAIT_OBJECT_0,
};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows::Win32::System::Console::{GetConsoleWindow, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, ResumeThread,
    STARTF_USESHOWWINDOW, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNORMAL};
use windows::core::{PCWSTR, PWSTR};

use super::{Io, Spec, Started, Window};
use crate::{Error, Result, err};

pub(crate) struct Handle {
    process: HANDLE,
    job: Option<HANDLE>,
}

// The handles are kernel handles, usable from any thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.process);
            if let Some(j) = self.job {
                let _ = CloseHandle(j);
            }
        }
    }
}

impl Handle {
    pub fn wait(&self, timeout: Option<Duration>) -> Option<Option<i32>> {
        let ms = timeout.map(|d| d.as_millis().min(u32::MAX as u128 - 1) as u32).unwrap_or(INFINITE);
        if unsafe { WaitForSingleObject(self.process, ms) } != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0u32;
        let _ = unsafe { GetExitCodeProcess(self.process, &mut code) };
        Some(Some(code as i32))
    }

    pub fn kill(&self) {
        unsafe {
            if let Some(j) = self.job
                && TerminateJobObject(j, 1).is_ok()
            {
                return;
            }
            let _ = TerminateProcess(self.process, 1);
        }
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn os_error(e: windows::core::Error) -> std::io::Error {
    let hr = e.code().0 as u32;
    // HRESULT_FROM_WIN32: 0x8007xxxx.
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        std::io::Error::from_raw_os_error((hr & 0xFFFF) as i32)
    } else {
        std::io::Error::other(e.message())
    }
}

/// Quotes an argument the way programs split their command line
/// (CommandLineToArgvW and the C runtime).
pub(crate) fn quote_arg(a: &str, out: &mut String) {
    if !a.is_empty() && !a.contains([' ', '\t', '\n', '\x0b', '"']) {
        out.push_str(a);
        return;
    }
    out.push('"');
    let mut slashes = 0;
    for c in a.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
}

/// Quotes an argument for cmd.exe: in double quotes, & | < > ^ ( ) are
/// plain characters. cmd.exe cannot take a double quote or a line break in
/// an argument; %NAME% is still expanded.
pub(crate) fn shell_quote(a: &str) -> Result<String> {
    if a.contains(['"', '\r', '\n']) {
        return Err(err!("process.badShellArg", a.to_string()));
    }
    if !a.is_empty() && !a.contains([' ', '\t', '&', '|', '<', '>', '^', '(', ')', ',', ';', '=', '!', '%']) {
        return Ok(a.to_string());
    }
    Ok(format!("\"{a}\""))
}

fn comspec() -> String {
    std::env::var("ComSpec").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "cmd.exe".into())
}

fn is_batch(p: &str) -> bool {
    let l = p.trim_end_matches([' ', '.']).to_ascii_lowercase();
    l.ends_with(".bat") || l.ends_with(".cmd")
}

/// The command line given to CreateProcessW.
pub(crate) fn command_line(spec: &Spec) -> Result<String> {
    if spec.shell || is_batch(&spec.command) {
        // cmd.exe /s /c "<line>": the outer quotes are removed and the
        // rest is run as typed.
        let mut line = if spec.shell { spec.command.clone() } else { shell_quote(&spec.command)? };
        for a in &spec.args {
            line.push(' ');
            line.push_str(&shell_quote(a)?);
        }
        let mut out = String::new();
        quote_arg(&comspec(), &mut out);
        out.push_str(" /d /s /c \"");
        out.push_str(&line);
        out.push('"');
        return Ok(out);
    }
    let mut out = String::new();
    quote_arg(&spec.command, &mut out);
    for a in &spec.args {
        out.push(' ');
        quote_arg(a, &mut out);
    }
    Ok(out)
}

/// The environment block (sorted, as Windows wants it) with the changes.
fn env_block(spec: &Spec) -> Option<Vec<u16>> {
    if spec.env.is_empty() {
        return None;
    }
    let mut vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    for (k, v) in &spec.env {
        let up = k.to_uppercase();
        vars.retain(|(n, _)| n.to_string_lossy().to_uppercase() != up);
        if let Some(v) = v {
            vars.push((k.into(), v.into()));
        }
    }
    vars.sort_by_key(|(k, _)| k.to_string_lossy().to_uppercase());
    let mut b = Vec::new();
    for (k, v) in vars {
        b.extend(k.encode_wide());
        b.push('=' as u16);
        b.extend(v.encode_wide());
        b.push(0);
    }
    b.push(0);
    Some(b)
}

/// Handles to close (the child's ends, after CreateProcessW).
struct Owned(Vec<HANDLE>);

impl Drop for Owned {
    fn drop(&mut self) {
        for h in &self.0 {
            unsafe {
                let _ = CloseHandle(*h);
            }
        }
    }
}

fn inheritable() -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: std::ptr::null_mut(), bInheritHandle: true.into() }
}

fn null_handle() -> Result<HANDLE> {
    let sa = inheritable();
    let name = wide(OsStr::new("NUL"));
    unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            Some(&sa),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| Error::from(os_error(e)))
}

/// The child's end of a standard stream, and the parent's end of its pipe.
fn child_io(io: Io, which: STD_HANDLE, child_reads: bool, owned: &mut Owned) -> Result<(HANDLE, Option<File>)> {
    match io {
        Io::Ignore => {
            let h = null_handle()?;
            owned.0.push(h);
            Ok((h, None))
        }
        Io::Inherit => {
            let src = unsafe { GetStdHandle(which) }.unwrap_or_default();
            if src.is_invalid() || src.0.is_null() {
                return child_io(Io::Ignore, which, child_reads, owned);
            }
            let mut dup = HANDLE::default();
            let me = unsafe { GetCurrentProcess() };
            if unsafe { DuplicateHandle(me, src, me, &mut dup, 0, true, DUPLICATE_SAME_ACCESS) }.is_err() {
                return child_io(Io::Ignore, which, child_reads, owned);
            }
            owned.0.push(dup);
            Ok((dup, None))
        }
        Io::Pipe => {
            let sa = inheritable();
            let (mut r, mut w) = (HANDLE::default(), HANDLE::default());
            unsafe { CreatePipe(&mut r, &mut w, Some(&sa), 0) }.map_err(|e| Error::from(os_error(e)))?;
            let (child, parent) = if child_reads { (r, w) } else { (w, r) };
            owned.0.push(child);
            let _ = unsafe { SetHandleInformation(parent, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) };
            Ok((child, Some(unsafe { File::from_raw_handle(parent.0 as _) })))
        }
    }
}

fn new_job() -> Option<HANDLE> {
    let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.ok()?;
    // Programs that ask to leave the job (CREATE_BREAKAWAY_FROM_JOB) may.
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_BREAKAWAY_OK;
    let _ = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    Some(job)
}

pub(crate) fn spawn(spec: &Spec) -> Result<Started> {
    let mut cmdline = wide(OsStr::new(&command_line(spec)?));
    let env = env_block(spec);
    let cwd = spec.cwd.as_ref().map(|d| wide(OsStr::new(d)));
    let mut owned = Owned(Vec::new());
    let (hin, stdin) = child_io(spec.stdin, STD_INPUT_HANDLE, true, &mut owned)?;
    let (hout, stdout) = child_io(spec.stdout, STD_OUTPUT_HANDLE, false, &mut owned)?;
    let (herr, stderr) = child_io(spec.stderr, STD_ERROR_HANDLE, false, &mut owned)?;

    let handles = [hin, hout, herr];
    let mut size = 0usize;
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
    let mut attr_buf = vec![0u8; size];
    let attrs = LPPROC_THREAD_ATTRIBUTE_LIST(attr_buf.as_mut_ptr() as *mut c_void);
    unsafe { InitializeProcThreadAttributeList(Some(attrs), 1, None, &mut size) }.map_err(|e| Error::from(os_error(e)))?;
    struct Attrs(LPPROC_THREAD_ATTRIBUTE_LIST);
    impl Drop for Attrs {
        fn drop(&mut self) {
            unsafe { DeleteProcThreadAttributeList(self.0) };
        }
    }
    let _attrs = Attrs(attrs);
    unsafe {
        UpdateProcThreadAttribute(
            attrs,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            Some(handles.as_ptr() as *const c_void),
            size_of_val(&handles),
            None,
            None,
        )
    }
    .map_err(|e| Error::from(os_error(e)))?;

    let mut si = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW { cb: size_of::<STARTUPINFOEXW>() as u32, ..Default::default() },
        lpAttributeList: attrs,
    };
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = hin;
    si.StartupInfo.hStdOutput = hout;
    si.StartupInfo.hStdError = herr;
    let has_console = !unsafe { GetConsoleWindow() }.0.is_null();
    let mut flags: PROCESS_CREATION_FLAGS = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED;
    let show = match spec.window {
        Window::Default => None,
        Window::Hidden => Some(SW_HIDE),
        Window::Normal => Some(SW_SHOWNORMAL),
        Window::Minimized => Some(SW_SHOWMINNOACTIVE),
        Window::Maximized => Some(SW_SHOWMAXIMIZED),
    };
    match spec.window {
        // In the console of xljs when it has one, else without a window.
        Window::Default if has_console => {}
        Window::Default | Window::Hidden => flags |= CREATE_NO_WINDOW,
        _ => flags |= CREATE_NEW_CONSOLE,
    }
    if let Some(sw) = show {
        si.StartupInfo.dwFlags |= STARTF_USESHOWWINDOW;
        si.StartupInfo.wShowWindow = sw.0 as u16;
    }
    let mut pi = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            true,
            flags,
            env.as_ref().map(|b| b.as_ptr() as *const c_void),
            cwd.as_ref().map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
            &si.StartupInfo,
            &mut pi,
        )
    }
    .map_err(|e| Error::path("exec", spec.command.clone(), os_error(e)))?;
    drop(owned);
    let job = new_job().filter(|j| {
        let ok = unsafe { AssignProcessToJobObject(*j, pi.hProcess) }.is_ok();
        if !ok {
            unsafe {
                let _ = CloseHandle(*j);
            }
        }
        ok
    });
    unsafe {
        ResumeThread(pi.hThread);
        let _ = CloseHandle(pi.hThread);
    }
    Ok(Started { pid: pi.dwProcessId, handle: Handle { process: pi.hProcess, job }, stdin, stdout, stderr })
}
