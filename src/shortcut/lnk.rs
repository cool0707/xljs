//! The shell link format (.lnk, MS-SHLLINK) read and written without the
//! shell: the header, the item ID list (read only), the link information,
//! the strings and the environment variable blocks. Windows builds its
//! links with IShellLinkW (`win.rs`); this serves the other platforms (the
//! tests on Linux). The Windows tests check that it reads what IShellLinkW
//! writes; the links it writes have no item ID list, and IShellLinkW gives
//! no target for them.

use super::Link;

const HEADER_SIZE: u32 = 0x4C;
const CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

const HAS_ID_LIST: u32 = 0x1;
const HAS_LINK_INFO: u32 = 0x2;
const HAS_NAME: u32 = 0x4;
const HAS_RELATIVE_PATH: u32 = 0x8;
const HAS_WORKING_DIR: u32 = 0x10;
const HAS_ARGUMENTS: u32 = 0x20;
const HAS_ICON_LOCATION: u32 = 0x40;
const IS_UNICODE: u32 = 0x80;
const HAS_EXP_STRING: u32 = 0x200;
const RUN_AS_USER: u32 = 0x2000;
const HAS_EXP_ICON: u32 = 0x4000;

const ENVIRONMENT_BLOCK: u32 = 0xA000_0001;
const ICON_ENVIRONMENT_BLOCK: u32 = 0xA000_0007;

struct Rd<'a> {
    b: &'a [u8],
}

impl Rd<'_> {
    fn u16(&self, at: usize) -> Option<u16> {
        Some(u16::from_le_bytes(self.b.get(at..at + 2)?.try_into().ok()?))
    }
    fn u32(&self, at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(self.b.get(at..at + 4)?.try_into().ok()?))
    }
    /// A NUL-terminated ANSI (CP932) string.
    fn ansi(&self, at: usize) -> Option<String> {
        let s = self.b.get(at..)?;
        let n = s.iter().position(|&c| c == 0)?;
        Some(
            encoding_rs::SHIFT_JIS
                .decode_without_bom_handling(&s[..n])
                .0
                .into_owned(),
        )
    }
    /// A NUL-terminated UTF-16 string.
    fn wide(&self, at: usize) -> Option<String> {
        let mut v = Vec::new();
        let mut i = at;
        loop {
            let c = self.u16(i)?;
            if c == 0 {
                break;
            }
            v.push(c);
            i += 2;
        }
        Some(String::from_utf16_lossy(&v))
    }
    /// A fixed-size NUL-padded UTF-16 field.
    fn wide_fixed(&self, at: usize, bytes: usize) -> Option<String> {
        let s = self.b.get(at..at + bytes)?;
        let v: Vec<u16> = s
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        Some(String::from_utf16_lossy(&v))
    }
}

/// Parses a .lnk file; None when it is not one.
pub fn parse(b: &[u8]) -> Option<Link> {
    let r = Rd { b };
    if r.u32(0)? != HEADER_SIZE || b.get(4..20)? != CLSID {
        return None;
    }
    let flags = r.u32(20)?;
    let mut l = Link {
        icon_index: r.u32(56)? as i32,
        show: r.u32(60)?,
        hotkey: r.u16(64)?,
        run_as_admin: flags & RUN_AS_USER != 0,
        ..Default::default()
    };
    let mut at = HEADER_SIZE as usize;
    let mut id_path = String::new();
    if flags & HAS_ID_LIST != 0 {
        let n = r.u16(at)? as usize;
        id_path = id_list_path(b.get(at + 2..at + 2 + n)?);
        at += 2 + n;
    }
    let (mut info_path, mut info_wide) = (String::new(), false);
    if flags & HAS_LINK_INFO != 0 {
        let n = r.u32(at)? as usize;
        (info_path, info_wide) = link_info_path(b.get(at..at + n)?).unwrap_or_default();
        at += n;
    }
    let unicode = flags & IS_UNICODE != 0;
    let string = |at: &mut usize| -> Option<String> {
        let n = r.u16(*at)? as usize;
        *at += 2;
        let s = if unicode {
            let s = b.get(*at..*at + n * 2)?;
            *at += n * 2;
            String::from_utf16_lossy(
                &s.chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<_>>(),
            )
        } else {
            let s = b.get(*at..*at + n)?;
            *at += n;
            encoding_rs::SHIFT_JIS
                .decode_without_bom_handling(s)
                .0
                .into_owned()
        };
        // Some writers count a terminating NUL.
        Some(s.trim_end_matches('\0').to_string())
    };
    let mut rel = String::new();
    for (bit, field) in [
        (HAS_NAME, &mut l.description),
        (HAS_RELATIVE_PATH, &mut rel),
        (HAS_WORKING_DIR, &mut l.cwd),
        (HAS_ARGUMENTS, &mut l.args),
        (HAS_ICON_LOCATION, &mut l.icon),
    ] {
        if flags & bit != 0 {
            *field = string(&mut at)?;
        }
    }
    let mut env_target = String::new();
    while let Some(size) = r.u32(at) {
        if size < 8 {
            break;
        }
        let sig = r.u32(at + 4)?;
        match sig {
            ENVIRONMENT_BLOCK if flags & HAS_EXP_STRING != 0 => env_target = block_string(&r, at)?,
            ICON_ENVIRONMENT_BLOCK if flags & HAS_EXP_ICON != 0 => {
                l.icon = super::expand_env(&block_string(&r, at)?)
            }
            _ => {}
        }
        at += size as usize;
    }
    l.target = if !env_target.is_empty() {
        super::expand_env(&env_target)
    } else if !info_path.is_empty() && (info_wide || id_path.is_empty()) {
        info_path
    } else if !id_path.is_empty() {
        id_path
    } else {
        info_path
    };
    Some(l)
}

/// The string of an environment variable block (Unicode, else ANSI).
fn block_string(r: &Rd, at: usize) -> Option<String> {
    let w = r.wide_fixed(at + 8 + 260, 520)?;
    if !w.is_empty() {
        Some(w)
    } else {
        r.ansi(at + 8)
    }
}

/// The target in the link information, and whether it was in Unicode (the
/// ANSI strings are read as CP932).
fn link_info_path(b: &[u8]) -> Option<(String, bool)> {
    let r = Rd { b };
    let header = r.u32(4)? as usize;
    let flags = r.u32(8)?;
    let wide = header >= 0x24;
    let mut unicode = true;
    let mut pick = |w: Option<u32>, a: u32| -> Option<String> {
        match w.filter(|&o| o != 0) {
            Some(o) => r.wide(o as usize),
            None => {
                let s = r.ansi(a as usize)?;
                unicode &= s.is_ascii();
                Some(s)
            }
        }
    };
    let suffix = pick(if wide { r.u32(0x20) } else { None }, r.u32(0x18)?)?;
    let base = if flags & 1 != 0 {
        pick(if wide { r.u32(0x1C) } else { None }, r.u32(0x10)?)?
    } else if flags & 2 != 0 {
        let at = r.u32(0x14)? as usize;
        let net_off = r.u32(at + 8)?;
        let net = match (net_off > 0x14)
            .then(|| r.u32(at + 0x14))
            .flatten()
            .filter(|&o| o != 0)
        {
            Some(o) => r.wide(at + o as usize)?,
            None => pick(None, at as u32 + net_off)?,
        };
        if suffix.is_empty() || net.ends_with('\\') {
            net
        } else {
            net + "\\"
        }
    } else {
        return None;
    };
    Some((base + &suffix, unicode))
}

/// The file system path of an item ID list (best effort: the drive and
/// the file entries' long names).
fn id_list_path(b: &[u8]) -> String {
    let r = Rd { b };
    let mut path = String::new();
    let mut volume = false;
    let mut at = 0;
    while let Some(n) = r.u16(at) {
        let n = n as usize;
        if n < 3 || at + n > b.len() {
            break;
        }
        let item = &b[at..at + n];
        let kind = item[2];
        if kind & 0x70 == 0x20 {
            // A volume: "C:\".
            if let Some(s) = (Rd { b: item }).ansi(3) {
                path = s;
                volume = true;
            }
        } else if volume
            && kind & 0x70 == 0x30
            && let Some(name) = file_entry_name(item)
        {
            if !path.is_empty() && !path.ends_with('\\') {
                path.push('\\');
            }
            path.push_str(&name);
        }
        at += n;
    }
    if volume { path } else { String::new() }
}

fn file_entry_name(item: &[u8]) -> Option<String> {
    let r = Rd { b: item };
    let unicode = item[2] & 0x04 != 0;
    let (short, mut end) = if unicode {
        let s = r.wide(14)?;
        (s.clone(), 14 + (s.encode_utf16().count() + 1) * 2)
    } else {
        let s = r.ansi(14)?;
        (s.clone(), 14 + s.len() + 1)
    };
    end += end % 2;
    // The extension block 0xBEEF0004 holds the long name.
    let mut at = end;
    while let (Some(size), Some(sig)) = (r.u16(at), r.u32(at + 4)) {
        let size = size as usize;
        if size < 8 {
            break;
        }
        if sig == 0xBEEF_0004 {
            let ver = r.u16(at + 2)?;
            let mut off = 18;
            if ver >= 7 {
                off += 18;
            }
            if ver >= 3 {
                off += 2;
            }
            if ver >= 9 {
                off += 4;
            }
            if ver >= 8 {
                off += 4;
            }
            if let Some(s) = r.wide(at + off).filter(|s| !s.is_empty()) {
                return Some(s);
            }
        }
        at += size;
    }
    Some(short)
}

// ---------------------------------------------------------------- writing

fn ansi(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4];
    for c in s.chars() {
        let (b, _, bad) = encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buf));
        if bad {
            out.push(b'?')
        } else {
            out.extend_from_slice(&b)
        }
    }
    out
}

fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put32_at(v: &mut [u8], at: usize, x: u32) {
    v[at..at + 4].copy_from_slice(&x.to_le_bytes());
}

/// The link information of a local path ("C:\dir\file") or a UNC path
/// ("\\server\share\dir\file").
fn link_info(target: &str) -> Vec<u8> {
    let mut v = vec![0u8; 0x24];
    put32_at(&mut v, 4, 0x24);
    if let Some(rest) = target.strip_prefix("\\\\") {
        // \\server\share is the net name, the rest the common path suffix.
        let mut parts = rest.splitn(3, '\\');
        let (server, share) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let net = format!("\\\\{server}\\{share}");
        let suffix = parts.next().unwrap_or("").to_string();
        put32_at(&mut v, 8, 2);
        let at = v.len() as u32;
        put32_at(&mut v, 0x14, at);
        let mut cnrl = vec![0u8; 0x1C];
        put32_at(&mut cnrl, 4, 2); // ValidNetType
        put32_at(&mut cnrl, 8, 0x1C);
        put32_at(&mut cnrl, 0x10, 0x0002_0000); // WNNC_NET_LANMAN
        cnrl.extend(ansi(&net));
        cnrl.push(0);
        let at = cnrl.len() as u32;
        put32_at(&mut cnrl, 0x14, at);
        cnrl.extend(wide(&net));
        cnrl.extend([0, 0]);
        let n = cnrl.len() as u32;
        put32_at(&mut cnrl, 0, n);
        v.extend(cnrl);
        let at = v.len() as u32;
        put32_at(&mut v, 0x18, at);
        v.extend(ansi(&suffix));
        v.push(0);
        let at = v.len() as u32;
        put32_at(&mut v, 0x20, at);
        v.extend(wide(&suffix));
        v.extend([0, 0]);
    } else {
        put32_at(&mut v, 8, 1);
        let at = v.len() as u32;
        put32_at(&mut v, 0xC, at);
        // VolumeID: a fixed drive without a label.
        let mut vol = vec![0u8; 0x10];
        put32_at(&mut vol, 0, 0x11);
        put32_at(&mut vol, 4, 3); // DRIVE_FIXED
        put32_at(&mut vol, 0xC, 0x10);
        vol.push(0);
        v.extend(vol);
        let at = v.len() as u32;
        put32_at(&mut v, 0x10, at);
        v.extend(ansi(target));
        v.push(0);
        let at = v.len() as u32;
        put32_at(&mut v, 0x18, at);
        v.push(0);
        let at = v.len() as u32;
        put32_at(&mut v, 0x1C, at);
        v.extend(wide(target));
        v.extend([0, 0]);
        let at = v.len() as u32;
        put32_at(&mut v, 0x20, at);
        v.extend([0, 0]);
    }
    let n = v.len() as u32;
    put32_at(&mut v, 0, n);
    v
}

fn env_block(sig: u32, s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(0x314);
    put32(&mut v, 0x314);
    put32(&mut v, sig);
    let mut a = ansi(s);
    a.resize(260, 0);
    a[259] = 0;
    v.extend(a);
    let mut w = wide(s);
    w.resize(520, 0);
    w[518] = 0;
    w[519] = 0;
    v.extend(w);
    v
}

/// Builds a .lnk file.
pub fn build(l: &Link) -> Vec<u8> {
    let env_target = l.target.contains('%');
    let env_icon = l.icon.contains('%');
    let mut flags = IS_UNICODE;
    let info = (!l.target.is_empty() && !env_target).then(|| link_info(&l.target));
    if info.is_some() {
        flags |= HAS_LINK_INFO;
    }
    let strings = [
        (HAS_NAME, &l.description),
        (HAS_WORKING_DIR, &l.cwd),
        (HAS_ARGUMENTS, &l.args),
        (HAS_ICON_LOCATION, &l.icon),
    ];
    for (bit, s) in strings {
        if !s.is_empty() {
            flags |= bit;
        }
    }
    if env_target {
        flags |= HAS_EXP_STRING;
    }
    if env_icon {
        flags |= HAS_EXP_ICON;
    }
    if l.run_as_admin {
        flags |= RUN_AS_USER;
    }
    let mut v = Vec::new();
    put32(&mut v, HEADER_SIZE);
    v.extend(CLSID);
    put32(&mut v, flags);
    put32(&mut v, 0); // file attributes
    v.extend([0u8; 24]); // creation, access and write times
    put32(&mut v, 0); // file size
    put32(&mut v, l.icon_index as u32);
    put32(
        &mut v,
        if l.show == 0 {
            super::SW_SHOWNORMAL
        } else {
            l.show
        },
    );
    v.extend(l.hotkey.to_le_bytes());
    v.extend([0u8; 10]);
    if let Some(info) = info {
        v.extend(info);
    }
    for (_, s) in strings {
        if !s.is_empty() {
            let w: Vec<u16> = s.encode_utf16().collect();
            v.extend((w.len() as u16).to_le_bytes());
            v.extend(w.iter().flat_map(|c| c.to_le_bytes()));
        }
    }
    if env_target {
        v.extend(env_block(ENVIRONMENT_BLOCK, &l.target));
    }
    if env_icon {
        v.extend(env_block(ICON_ENVIRONMENT_BLOCK, &l.icon));
    }
    put32(&mut v, 0);
    v
}
