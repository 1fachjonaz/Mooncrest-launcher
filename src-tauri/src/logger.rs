//! Plain-text log in `<data>/logs/launcher.log`, rotated at 1 MB (one old file kept).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());
const MAX: u64 = 1024 * 1024;

pub fn init(dir: PathBuf) {
    let _ = fs::create_dir_all(&dir);
    let _ = LOG_PATH.set(dir.join("launcher.log"));
}

pub fn path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}

fn write(level: &str, msg: &str) {
    let line = format!("{} [{level}] {msg}\n", crate::store::now_ms());
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    let Some(path) = LOG_PATH.get() else { return };
    let _g = LOCK.lock();
    if fs::metadata(path).map(|m| m.len() > MAX).unwrap_or(false) {
        let _ = fs::rename(path, path.with_extension("old.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn info(msg: &str) { write("INFO", msg) }
pub fn warn(msg: &str) { write("WARN", msg) }
pub fn error(msg: &str) { write("ERROR", msg) }
