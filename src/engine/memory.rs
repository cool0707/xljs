//! The free memory of the computer, for stopping a script before it uses
//! it up (an endless loop that keeps adding to an array grows by hundreds
//! of MB a second and makes the computer swap or fail long before the
//! watchdog asks). The check runs in the interrupt handler of watchdog.rs.

use std::time::Duration;

/// The free memory below which the CLI stops the script.
pub const DEFAULT_FLOOR: u64 = 100 << 20;

/// How often the interrupt handler reads the free memory.
pub(crate) const CHECK_EVERY: Duration = Duration::from_millis(50);

/// The physical memory available to programs now, in bytes; None when it
/// cannot be read.
#[cfg(windows)]
pub fn available() -> Option<u64> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut s = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe { GlobalMemoryStatusEx(&mut s) }.ok()?;
    Some(s.ullAvailPhys)
}

/// The physical memory available to programs now, in bytes (MemAvailable
/// of /proc/meminfo); None when it cannot be read.
#[cfg(not(windows))]
pub fn available() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_meminfo(&text)
}

#[cfg(not(windows))]
fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb: u64 = line["MemAvailable:".len()..]
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    Some(kb * 1024)
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_available_memory() {
        let a = super::available().expect("available memory");
        assert!(a > 0);
    }

    #[cfg(not(windows))]
    #[test]
    fn parses_meminfo() {
        let text = "MemTotal:       16000000 kB\nMemFree:  100 kB\nMemAvailable:   1234 kB\n";
        assert_eq!(super::parse_meminfo(text), Some(1234 * 1024));
        assert_eq!(super::parse_meminfo("MemTotal: 1 kB\n"), None);
    }
}
