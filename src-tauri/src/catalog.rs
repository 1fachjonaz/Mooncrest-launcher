//! The product catalog ("manifest") and news feed.
//!
//! Every product the launcher knows comes from `products.json`. The launcher
//! ships with a copy (compiled in) and refreshes it from the online catalog URL.
//! A newer valid online copy is cached on disk, so the launcher also works offline.
//! Adding a game, mod or tool = adding one entry with an `install` spec.

use crate::error::{AppError, AppResult};
use crate::store;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::Duration;

pub const BUNDLED_PRODUCTS: &str = include_str!("../resources/products.json");
pub const BUNDLED_NEWS: &str = include_str!("../resources/news.json");

/// Text in several languages: `{ "de": "...", "en": "..." }`.
pub type Localized<T> = BTreeMap<String, T>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    #[serde(default)]
    pub generated: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub launcher: Option<LauncherInfo>,
    pub products: Vec<Product>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherInfo {
    pub latest: Release,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProductKind {
    Game,
    Software,
    Mod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub kind: ProductKind,
    #[serde(default)]
    pub featured: bool,
    /// "in_development" for announced products without a download yet.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub developer: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub support_url: Option<String>,
    #[serde(default)]
    pub community_url: Option<String>,
    #[serde(default)]
    pub tagline: Localized<String>,
    #[serde(default)]
    pub description: Localized<Vec<String>>,
    #[serde(default)]
    pub highlights: Localized<Vec<String>>,
    #[serde(default)]
    pub includes: Vec<String>,
    #[serde(default)]
    pub requirements: Localized<Vec<String>>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub media: Media,
    pub install: InstallSpec,
    pub releases: Vec<Release>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub icon: String,
    /// Portrait cover (3:4) for shelves and the library grid.
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    pub hero: String,
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub screenshots: Vec<Screenshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshot {
    pub src: String,
    #[serde(default)]
    pub caption: Localized<String>,
}

/// How a product gets onto the disk. New installer kinds go here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallSpec {
    /// A Windows NSIS setup (.exe), run silently. Detected through the
    /// uninstall entry `HKCU|HKLM\...\Uninstall\<registry_name>`.
    Nsis {
        registry_name: String,
        folder: String,
        main_binary: String,
        #[serde(default = "default_silent")]
        silent_args: Vec<String>,
        #[serde(default = "default_update")]
        update_args: Vec<String>,
    },
    /// A portable archive extracted into `<library>\<folder>`.
    Zip {
        folder: String,
        main_binary: String,
        #[serde(default)]
        strip_root: bool,
    },
    /// A single file (for example a mod .jar) copied into `<library>\<folder>`.
    File { folder: String },
}

fn default_silent() -> Vec<String> { vec!["/S".into()] }
fn default_update() -> Vec<String> { vec!["/S".into(), "/UPDATE".into()] }

impl InstallSpec {
    pub fn folder(&self) -> &str {
        match self {
            InstallSpec::Nsis { folder, .. } | InstallSpec::Zip { folder, .. } | InstallSpec::File { folder } => folder,
        }
    }
    pub fn main_binary(&self) -> Option<&str> {
        match self {
            InstallSpec::Nsis { main_binary, .. } | InstallSpec::Zip { main_binary, .. } => Some(main_binary),
            InstallSpec::File { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// RFC 3339 time before which the download is announced but not offered.
    #[serde(default)]
    pub available_from: Option<String>,
    #[serde(default)]
    pub files: Vec<ReleaseFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseFile {
    pub platform: String,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

pub const PLATFORM: &str = if cfg!(target_os = "windows") {
    "windows-x86_64"
} else if cfg!(target_os = "linux") {
    "linux-x86_64"
} else {
    "other"
};

impl Release {
    pub fn file_for_platform(&self) -> Option<&ReleaseFile> {
        self.files.iter().find(|f| f.platform == PLATFORM)
    }
}

impl Product {
    /// Newest release (by semver) that has a file for this platform.
    pub fn latest(&self) -> Option<&Release> {
        self.releases
            .iter()
            .filter(|r| r.file_for_platform().is_some())
            .max_by(|a, b| compare_versions(&a.version, &b.version))
    }
}

/// Semver comparison with a lenient fallback for non-semver strings.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = semver::Version::parse(a.trim_start_matches('v'));
    let pb = semver::Version::parse(b.trim_start_matches('v'));
    match (pa, pb) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

pub fn is_newer(candidate: &str, installed: &str) -> bool {
    compare_versions(candidate, installed) == std::cmp::Ordering::Greater
}

fn valid_download_url(u: &str) -> bool {
    if u.starts_with("https://") {
        return true;
    }
    // Plain http only for a local test server in debug builds.
    cfg!(debug_assertions) && (u.starts_with("http://127.0.0.1") || u.starts_with("http://localhost"))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Manifest {
    pub fn parse(text: &str) -> AppResult<Manifest> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| AppError::InvalidManifest(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.schema != 1 {
            return Err(AppError::InvalidManifest(format!("unsupported schema {}", self.schema)));
        }
        let mut ids = HashSet::new();
        for p in &self.products {
            if !valid_id(&p.id) {
                return Err(AppError::InvalidManifest(format!("bad product id '{}'", p.id)));
            }
            if !ids.insert(&p.id) {
                return Err(AppError::InvalidManifest(format!("duplicate product id '{}'", p.id)));
            }
            store::safe_file_name(p.install.folder()).map_err(|_| AppError::InvalidManifest(format!("bad folder for {}", p.id)))?;
            if let Some(bin) = p.install.main_binary() {
                store::safe_file_name(bin).map_err(|_| AppError::InvalidManifest(format!("bad main_binary for {}", p.id)))?;
            }
            for r in &p.releases {
                validate_files(&p.id, r)?;
            }
        }
        if let Some(l) = &self.launcher {
            validate_files("launcher", &l.latest)?;
        }
        Ok(())
    }

    pub fn product(&self, id: &str) -> AppResult<&Product> {
        self.products.iter().find(|p| p.id == id).ok_or_else(|| AppError::Invalid(format!("unknown product '{id}'")))
    }
}

fn validate_files(owner: &str, r: &Release) -> AppResult<()> {
    if r.version.trim().is_empty() {
        return Err(AppError::InvalidManifest(format!("{owner}: empty version")));
    }
    for f in &r.files {
        let hash_ok = f.sha256.len() == 64 && f.sha256.chars().all(|c| c.is_ascii_hexdigit());
        if !hash_ok {
            return Err(AppError::InvalidManifest(format!("{owner} {}: bad sha256", r.version)));
        }
        if f.size == 0 {
            return Err(AppError::InvalidManifest(format!("{owner} {}: size missing", r.version)));
        }
        if !valid_download_url(&f.url) {
            return Err(AppError::InvalidManifest(format!("{owner} {}: download URL must use https", r.version)));
        }
        store::safe_file_name(&f.file_name).map_err(|_| AppError::InvalidManifest(format!("{owner} {}: bad file name", r.version)))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- news

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsFeed {
    pub schema: u32,
    pub entries: Vec<NewsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsEntry {
    pub id: String,
    pub date: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    pub title: Localized<String>,
    #[serde(default)]
    pub summary: Localized<String>,
}

impl NewsFeed {
    pub fn parse(text: &str) -> AppResult<NewsFeed> {
        let mut f: NewsFeed = serde_json::from_str(text).map_err(|e| AppError::InvalidManifest(e.to_string()))?;
        if f.schema != 1 {
            return Err(AppError::InvalidManifest("unsupported news schema".into()));
        }
        // Only https links are opened from news entries.
        for e in &mut f.entries {
            if e.url.as_deref().is_some_and(|u| !u.starts_with("https://")) {
                e.url = None;
            }
        }
        f.entries.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(f)
    }
}

// ---------------------------------------------------------------- loading

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Fresh from the online catalog.
    Online,
    /// From the last successful online refresh (offline now).
    Cache,
    /// Built into this launcher version.
    Bundled,
}

/// News lives next to the catalog: `.../products.json` → `.../news.json`.
pub fn news_url_for(catalog_url: &str) -> Option<String> {
    let idx = catalog_url.rfind('/')?;
    Some(format!("{}/news.json", &catalog_url[..idx]))
}

pub async fn fetch_text(client: &reqwest::Client, url: &str) -> AppResult<String> {
    let resp = client.get(url).timeout(Duration::from_secs(15)).send().await?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Err(AppError::NotFound);
    }
    if !status.is_success() {
        return Err(AppError::HttpStatus(status.as_u16()));
    }
    let ctype = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if ctype.contains("text/html") {
        return Err(AppError::NotAvailable("server returned a web page instead of data".into()));
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(AppError::InvalidManifest("catalog is too large".into()));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| AppError::InvalidManifest("not UTF-8".into()))
}

/// The best catalog available right now: a cached online copy if it is valid
/// and not older than the bundled one, otherwise the bundled copy.
pub fn load_local(cache_file: &Path) -> (Manifest, Source) {
    let bundled = Manifest::parse(BUNDLED_PRODUCTS).expect("bundled catalog must be valid");
    if let Ok(text) = std::fs::read_to_string(cache_file) {
        match Manifest::parse(&text) {
            Ok(m) if !older_than(&m, &bundled) => return (m, Source::Cache),
            Ok(_) => {}
            Err(e) => crate::logger::warn(&format!("cached catalog ignored: {e}")),
        }
    }
    (bundled, Source::Bundled)
}

fn older_than(a: &Manifest, b: &Manifest) -> bool {
    match (&a.generated, &b.generated) {
        (Some(x), Some(y)) => x < y,
        _ => false,
    }
}

pub fn load_local_news(cache_file: &Path) -> (NewsFeed, Source) {
    if let Ok(text) = std::fs::read_to_string(cache_file) {
        if let Ok(f) = NewsFeed::parse(&text) {
            return (f, Source::Cache);
        }
    }
    (NewsFeed::parse(BUNDLED_NEWS).expect("bundled news must be valid"), Source::Bundled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_valid() {
        let m = Manifest::parse(BUNDLED_PRODUCTS).unwrap();
        let p = m.product("mlbclient").unwrap();
        assert_eq!(p.releases.len(), 2);
        let c = m.product("city-life").unwrap();
        assert!(c.featured && c.latest().is_none(), "City Life is announced, not downloadable yet");
        assert!(NewsFeed::parse(BUNDLED_NEWS).unwrap().entries.len() >= 5);
    }

    #[test]
    fn version_ordering() {
        assert!(is_newer("0.3.0-beta", "0.2.0-beta"));
        assert!(is_newer("1.0.0", "1.0.0-beta"));
        assert!(!is_newer("0.2.0-beta", "0.3.0-beta"));
        assert!(!is_newer("0.3.0-beta", "0.3.0-beta"));
    }

    #[test]
    fn rejects_bad_hash_and_http() {
        let mut m = Manifest::parse(BUNDLED_PRODUCTS).unwrap();
        m.products[1].releases[0].files[0].sha256 = "abc".into();
        assert!(m.validate().is_err());
        let mut m = Manifest::parse(BUNDLED_PRODUCTS).unwrap();
        m.products[1].releases[0].files[0].url = "http://example.com/x.exe".into();
        assert!(m.validate().is_err());
        let mut m = Manifest::parse(BUNDLED_PRODUCTS).unwrap();
        m.products[1].releases[0].files[0].file_name = "..\\evil.exe".into();
        assert!(m.validate().is_err());
    }

    #[test]
    fn news_url() {
        assert_eq!(news_url_for("https://a.b/x/v1/products.json").unwrap(), "https://a.b/x/v1/news.json");
    }
}
