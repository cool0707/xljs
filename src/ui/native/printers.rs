//! The printers of this computer (Ui::printers): local and network
//! printers (EnumPrintersW, level 4), the default one first, with whether
//! they print on both sides and in color (DeviceCapabilitiesW).

use windows::Win32::Graphics::Printing::{EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W};
use windows::Win32::Storage::Xps::{DC_COLORDEVICE, DC_DUPLEX, DeviceCapabilitiesW};
use windows::core::{PCWSTR, PWSTR};

use super::hstr;
use crate::ui::PrinterInfo;

/// The name of the default printer ("" if there is none).
pub(super) fn default_printer() -> String {
    unsafe {
        let mut n = 0u32;
        let _ = GetDefaultPrinterW(None, &mut n);
        if n == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; n as usize];
        if !GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &mut n).as_bool() {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(buf.len())])
    }
}

pub(super) fn list() -> Vec<PrinterInfo> {
    let mut names: Vec<String> = Vec::new();
    unsafe {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut needed, mut count) = (0u32, 0u32);
        let _ = EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut count);
        if needed > 0 {
            // u64 for the alignment of the records.
            let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
            let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * 8);
            if EnumPrintersW(flags, PCWSTR::null(), 4, Some(bytes), &mut needed, &mut count).is_ok() {
                let infos = std::slice::from_raw_parts(buf.as_ptr() as *const PRINTER_INFO_4W, count as usize);
                for i in infos {
                    if let Ok(name) = i.pPrinterName.to_string()
                        && !name.is_empty()
                        && !names.contains(&name)
                    {
                        names.push(name);
                    }
                }
            }
        }
    }
    let def = default_printer();
    let mut out: Vec<PrinterInfo> = names
        .into_iter()
        .map(|name| {
            let h = hstr(&name);
            let (duplex, color) = unsafe {
                (
                    DeviceCapabilitiesW(&h, PCWSTR::null(), DC_DUPLEX, None, None) == 1,
                    DeviceCapabilitiesW(&h, PCWSTR::null(), DC_COLORDEVICE, None, None) == 1,
                )
            };
            PrinterInfo { is_default: name == def, name, duplex, color }
        })
        .collect();
    out.sort_by_key(|p| !p.is_default);
    out
}
