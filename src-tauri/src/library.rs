//! What is installed where, plus play statistics.
//!
//! Records are kept in `library.json`. They are re-checked against reality on
//! every scan: the Windows uninstall entry for NSIS products (so an install or
//! update done outside the launcher is picked up) and the files on disk.

use crate::catalog::{InstallSpec, Product};
use crate::error::{AppError, AppResult};
use crate::{platform, store};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    /// Installed by the Mooncrest Launcher.
    Managed,
    /// Found on this PC (installed some other way).
    Detected,
    /// Selected by the user.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRecord {
    pub product_id: String,
    pub version: Option<String>,
    pub path: PathBuf,
    pub source: InstallSource,
    pub installed_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayStats {
    pub last_played: Option<u64>,
    pub play_secs: u64,
    pub launches: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    installs: HashMap<String, InstallRecord>,
    #[serde(default)]
    stats: HashMap<String, PlayStats>,
}

pub struct Library {
    file: PathBuf,
    data: Mutex<Data>,
}

/// Marker written into folders the launcher extracts, so uninstall never
/// deletes a folder it did not create.
pub const MARKER: &str = ".mooncrest.json";

impl Library {
    pub fn open(file: PathBuf) -> Library {
        let data = store::read_json(&file).ok().flatten().unwrap_or_default();
        Library { file, data: Mutex::new(data) }
    }

    fn save(&self, d: &Data) {
        if let Err(e) = store::write_json(&self.file, d) {
            crate::logger::error(&format!("saving library failed: {e}"));
        }
    }

    pub fn get(&self, id: &str) -> Option<InstallRecord> {
        self.data.lock().unwrap().installs.get(id).cloned()
    }

    pub fn stats(&self, id: &str) -> PlayStats {
        self.data.lock().unwrap().stats.get(id).cloned().unwrap_or_default()
    }

    pub fn put(&self, rec: InstallRecord) {
        let mut d = self.data.lock().unwrap();
        d.installs.insert(rec.product_id.clone(), rec);
        self.save(&d);
    }

    pub fn remove(&self, id: &str) {
        let mut d = self.data.lock().unwrap();
        if d.installs.remove(id).is_some() {
            self.save(&d);
        }
    }

    pub fn record_launch(&self, id: &str) {
        let mut d = self.data.lock().unwrap();
        let s = d.stats.entry(id.to_string()).or_default();
        s.launches += 1;
        s.last_played = Some(store::now_ms());
        self.save(&d);
    }

    pub fn add_playtime(&self, id: &str, secs: u64) {
        let mut d = self.data.lock().unwrap();
        let s = d.stats.entry(id.to_string()).or_default();
        s.play_secs += secs;
        s.last_played = Some(store::now_ms());
        self.save(&d);
    }

    /// Clears play statistics. Install records are kept.
    pub fn reset(&self) {
        let mut d = self.data.lock().unwrap();
        d.stats.clear();
        self.save(&d);
    }

    /// Re-checks one product against the system and returns the current record.
    pub fn scan(&self, product: &Product) -> Option<InstallRecord> {
        let found = detect(product, self.get(&product.id).as_ref());
        let mut d = self.data.lock().unwrap();
        let changed = match (&found, d.installs.get(&product.id)) {
            (Some(a), Some(b)) => a.version != b.version || a.path != b.path || a.source != b.source,
            (None, None) => false,
            _ => true,
        };
        if changed {
            match &found {
                Some(r) => {
                    d.installs.insert(product.id.clone(), r.clone());
                }
                None => {
                    d.installs.remove(&product.id);
                }
            }
            self.save(&d);
        }
        found
    }
}

/// Where the executable of an installed product is.
pub fn main_exe(product: &Product, rec: &InstallRecord) -> Option<PathBuf> {
    product.install.main_binary().map(|b| rec.path.join(b))
}

fn detect(product: &Product, known: Option<&InstallRecord>) -> Option<InstallRecord> {
    let now = store::now_ms();
    match &product.install {
        InstallSpec::Nsis { registry_name, main_binary, .. } => {
            if let Some(entry) = platform::uninstall_entry(registry_name) {
                let path = entry
                    .location
                    .clone()
                    .or_else(|| entry.uninstaller.as_ref().and_then(|u| u.parent().map(Path::to_path_buf)));
                if let Some(path) = path {
                    let bin = entry.main_binary.clone().unwrap_or_else(|| main_binary.clone());
                    if path.join(&bin).is_file() {
                        let source = match known {
                            Some(k) if k.source == InstallSource::Managed && same_path(&k.path, &path) => InstallSource::Managed,
                            _ => InstallSource::Detected,
                        };
                        let version = entry.version.or_else(|| platform::exe_version(&path.join(&bin)));
                        return Some(InstallRecord {
                            product_id: product.id.clone(),
                            version,
                            installed_at: known.map(|k| k.installed_at).unwrap_or(now),
                            updated_at: now,
                            path,
                            source,
                        });
                    }
                }
            }
            // No uninstall entry (e.g. copied by hand): keep a manual record while the exe exists.
            let k = known?;
            let exe = k.path.join(main_binary);
            if !exe.is_file() {
                return None;
            }
            let mut r = k.clone();
            if let Some(v) = platform::exe_version(&exe) {
                r.version = Some(v);
            }
            Some(r)
        }
        InstallSpec::Zip { main_binary, .. } => {
            let k = known?;
            if k.path.join(main_binary).is_file() { Some(k.clone()) } else { None }
        }
        InstallSpec::File { .. } => {
            let k = known?;
            if k.path.is_file() { Some(k.clone()) } else { None }
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    norm(a) == norm(b)
}

/// Validates a folder the user says contains an existing installation.
pub fn locate(product: &Product, dir: &Path) -> AppResult<InstallRecord> {
    let now = store::now_ms();
    match &product.install {
        InstallSpec::Nsis { main_binary, .. } | InstallSpec::Zip { main_binary, .. } => {
            let exe = dir.join(main_binary);
            if !exe.is_file() {
                return Err(AppError::Invalid(format!("{main_binary} was not found in this folder")));
            }
            let marker_version = read_marker(dir);
            Ok(InstallRecord {
                product_id: product.id.clone(),
                version: platform::exe_version(&exe).or(marker_version),
                path: dir.to_path_buf(),
                source: InstallSource::Manual,
                installed_at: now,
                updated_at: now,
            })
        }
        InstallSpec::File { .. } => Err(AppError::Unsupported),
    }
}

#[derive(Serialize, Deserialize)]
pub struct Marker {
    pub product_id: String,
    pub version: String,
}

pub fn read_marker(dir: &Path) -> Option<String> {
    let m: Marker = store::read_json(&dir.join(MARKER)).ok().flatten()?;
    Some(m.version)
}
