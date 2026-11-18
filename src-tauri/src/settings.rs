//! User settings, stored as JSON in the app config folder and validated on save.

use crate::error::{AppError, AppResult};
use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchBehavior {
    /// Keep the launcher window as it is.
    Keep,
    /// Minimize the launcher window.
    Minimize,
    /// Hide to the tray until the product exits.
    Tray,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub schema: u32,
    /// Where installers are downloaded to.
    pub download_dir: PathBuf,
    /// Library root: new installs go to `<install_dir>\<product folder>`.
    pub install_dir: PathBuf,
    pub auto_update_products: bool,
    pub auto_update_launcher: bool,
    /// Keep installer files after a successful install (otherwise they are deleted).
    pub keep_installers: bool,
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub on_product_launch: LaunchBehavior,
    /// "de" or "en".
    pub language: String,
    /// "night", "midnight" or "dawn".
    pub theme: String,
    pub reduce_motion: bool,
    pub notifications: bool,
    pub notify_downloads: bool,
    pub notify_updates: bool,
    /// Online catalog. Empty = use only the catalog built into the launcher.
    pub catalog_url: String,
}

pub const DEFAULT_CATALOG_URL: &str = "https://mlbclient.vercel.app/mooncrest/v1/products.json";

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema: 1,
            download_dir: PathBuf::new(),
            install_dir: PathBuf::new(),
            auto_update_products: true,
            auto_update_launcher: true,
            keep_installers: false,
            launch_on_startup: false,
            start_minimized: false,
            close_to_tray: false,
            on_product_launch: LaunchBehavior::Minimize,
            language: "de".into(),
            theme: "night".into(),
            reduce_motion: false,
            notifications: true,
            notify_downloads: true,
            notify_updates: true,
            catalog_url: DEFAULT_CATALOG_URL.into(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path, defaults: &Settings) -> Settings {
        let mut s: Settings = store::read_json(path).ok().flatten().unwrap_or_else(|| defaults.clone());
        if s.download_dir.as_os_str().is_empty() {
            s.download_dir = defaults.download_dir.clone();
        }
        if s.install_dir.as_os_str().is_empty() {
            s.install_dir = defaults.install_dir.clone();
        }
        if s.validate().is_err() {
            crate::logger::warn("settings failed validation, falling back to defaults for invalid fields");
            if !["de", "en"].contains(&s.language.as_str()) { s.language = "de".into() }
            if !["night", "midnight", "dawn"].contains(&s.theme.as_str()) { s.theme = "night".into() }
            if !valid_url(&s.catalog_url) { s.catalog_url = DEFAULT_CATALOG_URL.into() }
        }
        s
    }

    pub fn validate(&self) -> AppResult<()> {
        if !["de", "en"].contains(&self.language.as_str()) {
            return Err(AppError::Invalid("language must be de or en".into()));
        }
        if !["night", "midnight", "dawn"].contains(&self.theme.as_str()) {
            return Err(AppError::Invalid("unknown theme".into()));
        }
        for (label, p) in [("download_dir", &self.download_dir), ("install_dir", &self.install_dir)] {
            if !p.is_absolute() {
                return Err(AppError::Invalid(format!("{label} must be an absolute path")));
            }
        }
        if !valid_url(&self.catalog_url) {
            return Err(AppError::Invalid("catalog URL must start with https://".into()));
        }
        Ok(())
    }
}

fn valid_url(u: &str) -> bool {
    let scheme_ok = u.starts_with("https://")
        // Plain http only for a local test server in debug builds.
        || (cfg!(debug_assertions) && (u.starts_with("http://127.0.0.1") || u.starts_with("http://localhost")));
    u.is_empty() || (scheme_ok && u.len() < 2048 && !u.contains(char::is_whitespace))
}
