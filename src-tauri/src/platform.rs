//! Operating-system specific bits: registry lookup of installed programs,
//! executable version info, "is this exe running" and process flags.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct UninstallEntry {
    pub version: Option<String>,
    pub location: Option<PathBuf>,
    pub main_binary: Option<String>,
    pub uninstaller: Option<PathBuf>,
}

#[cfg(windows)]
fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_string()
}

/// Reads `...\CurrentVersion\Uninstall\<name>` from HKCU, then HKLM (64- and 32-bit views).
#[cfg(windows)]
pub fn uninstall_entry(name: &str) -> Option<UninstallEntry> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    use winreg::RegKey;
    let path = format!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{name}");
    let tries = [
        (HKEY_CURRENT_USER, KEY_READ),
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_READ | KEY_WOW64_32KEY),
    ];
    for (root, flags) in tries {
        let Ok(key) = RegKey::predef(root).open_subkey_with_flags(&path, flags) else { continue };
        let get = |v: &str| key.get_value::<String, _>(v).ok().map(|s| unquote(&s)).filter(|s| !s.is_empty());
        let entry = UninstallEntry {
            version: get("DisplayVersion"),
            location: get("InstallLocation").map(PathBuf::from),
            main_binary: get("MainBinaryName"),
            uninstaller: get("UninstallString").map(PathBuf::from),
        };
        if entry.location.is_some() || entry.uninstaller.is_some() {
            return Some(entry);
        }
    }
    None
}

#[cfg(not(windows))]
pub fn uninstall_entry(_name: &str) -> Option<UninstallEntry> {
    None
}

/// Product version from an executable's version resource.
#[cfg(windows)]
pub fn exe_version(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr() as *mut _) == 0 {
            return None;
        }
        // First translation entry, then its ProductVersion string.
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        let q: Vec<u16> = "\\VarFileInfo\\Translation".encode_utf16().chain(std::iter::once(0)).collect();
        if VerQueryValueW(buf.as_ptr() as *const _, q.as_ptr(), &mut ptr, &mut len) == 0 || len < 4 {
            return None;
        }
        let lang = *(ptr as *const u16);
        let cp = *((ptr as *const u16).add(1));
        let key = format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\ProductVersion");
        let q: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
        if VerQueryValueW(buf.as_ptr() as *const _, q.as_ptr(), &mut ptr, &mut len) == 0 || len == 0 {
            return None;
        }
        let slice = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let s = String::from_utf16_lossy(slice).trim_end_matches('\0').trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

#[cfg(not(windows))]
pub fn exe_version(_path: &Path) -> Option<String> {
    None
}

/// Windows locks the image of a running .exe against writing. Opening it for
/// write therefore tells us whether the program is running, without any extra
/// process-listing dependency.
#[cfg(windows)]
pub fn exe_in_use(path: &Path) -> bool {
    match std::fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => false,
        // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION
        Err(e) => matches!(e.raw_os_error(), Some(32) | Some(33)),
    }
}

#[cfg(not(windows))]
pub fn exe_in_use(_path: &Path) -> bool {
    false
}

/// A `Command` that never flashes a console window.
pub fn command(program: &Path) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Appends an argument exactly as written (no quoting). NSIS needs `/D=` and
/// `_?=` unquoted and last, even when the path contains spaces.
pub fn raw_arg(c: &mut std::process::Command, arg: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(arg);
    }
    #[cfg(not(windows))]
    {
        c.arg(arg);
    }
}
