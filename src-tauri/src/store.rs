//! Small helpers for files the launcher owns: atomic JSON writes, time stamps,
//! directory sizes and free disk space.

use crate::error::{AppError, AppResult};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Reads a JSON file. A missing file gives `Ok(None)`. A broken file is moved
/// aside as `<name>.corrupt` (so nothing is lost) and also gives `Ok(None)`.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            crate::logger::warn(&format!("{} is unreadable ({e}); keeping a copy and using defaults", path.display()));
            let mut aside = path.as_os_str().to_owned();
            aside.push(".corrupt");
            let _ = fs::rename(path, PathBuf::from(aside));
            Ok(None)
        }
    }
}

/// Writes JSON atomically: write to a temp file in the same folder, flush to
/// disk, then rename over the target. A crash never leaves a half-written file.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let dir = path.parent().ok_or_else(|| AppError::Io("invalid path".into()))?;
    fs::create_dir_all(dir)?;
    let data = serde_json::to_vec_pretty(value)?;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}-{n}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let Ok(rd) = fs::read_dir(path) else { return 0 };
    for entry in rd.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += meta.len();
        }
    }
    total
}

/// Free bytes on the volume that holds `path` (or its nearest existing parent).
pub fn free_space(path: &Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            return None;
        }
    }
    free_space_impl(&p)
}

#[cfg(windows)]
fn free_space_impl(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut avail: u64 = 0;
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, std::ptr::null_mut(), std::ptr::null_mut()) };
    if ok != 0 { Some(avail) } else { None }
}

#[cfg(not(windows))]
fn free_space_impl(path: &Path) -> Option<u64> {
    // `df` is available on every Linux/macOS desktop; good enough for a non-primary platform.
    let out = std::process::Command::new("df").arg("-Pk").arg(path).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let avail_kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kb * 1024)
}

/// True when the launcher can create files in `dir` (creates the folder if needed).
pub fn is_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".mooncrest-write-test-{}", now_ms()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Rejects file names that could escape the target folder.
pub fn safe_file_name(name: &str) -> AppResult<String> {
    let ok = !name.is_empty()
        && name.len() < 200
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && name != "."
        && name != ".."
        && !name.starts_with('.');
    if ok { Ok(name.to_string()) } else { Err(AppError::Invalid(format!("unsafe file name: {name}"))) }
}
