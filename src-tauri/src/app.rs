//! Application state and the operations behind the commands: catalog refresh,
//! install/update flow, launching, self-update and housekeeping.

use crate::catalog::{self, Manifest, NewsFeed, Product, Source};
use crate::download::{Engine, Hooks, Job, JobKind, JobState, NewJob};
use crate::error::{AppError, AppResult, ErrorInfo};
use crate::install::{self, InstallRequest};
use crate::library::{self, InstallRecord, Library, PlayStats};
use crate::settings::{LaunchBehavior, Settings};
use crate::{logger, platform, store};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const LAUNCHER_PRODUCT_ID: &str = "mooncrest-launcher";

pub struct Paths {
    pub base: PathBuf,
    pub settings: PathBuf,
    pub library: PathBuf,
    pub jobs: PathBuf,
    pub profile: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub default_downloads: PathBuf,
    pub default_library: PathBuf,
}

impl Paths {
    pub fn new(local_data: PathBuf) -> Paths {
        let root = local_data.join("Mooncrest");
        let base = root.join("Launcher");
        Paths {
            settings: base.join("settings.json"),
            library: base.join("library.json"),
            jobs: base.join("downloads.json"),
            profile: base.join("profile.json"),
            cache: base.join("cache"),
            logs: base.join("logs"),
            default_downloads: root.join("Downloads"),
            default_library: root.join("Library"),
            base,
        }
    }
    pub fn catalog_cache(&self) -> PathBuf {
        self.cache.join("products.json")
    }
    pub fn news_cache(&self) -> PathBuf {
        self.cache.join("news.json")
    }
}

#[derive(Clone, Serialize)]
pub struct CatalogState {
    pub manifest: Manifest,
    pub source: Source,
    pub fetched_at: Option<u64>,
    pub error: Option<ErrorInfo>,
}

#[derive(Clone, Serialize)]
pub struct NewsState {
    pub feed: NewsFeed,
    pub source: Source,
    pub fetched_at: Option<u64>,
    pub error: Option<ErrorInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub display_name: String,
    pub created_at: u64,
}

struct Running {
    started: Instant,
}

pub struct App {
    pub handle: AppHandle,
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub catalog: RwLock<CatalogState>,
    pub news: RwLock<NewsState>,
    pub library: Library,
    pub engine: Engine,
    pub client: reqwest::Client,
    running: Mutex<HashMap<String, Running>>,
    /// Set when the launcher hid/minimized itself for a running product.
    hidden_for_product: Mutex<bool>,
}

#[derive(Serialize)]
pub struct LibraryItem {
    pub product_id: String,
    pub install: Option<InstallRecord>,
    pub latest: Option<String>,
    pub update_available: bool,
    pub available: bool,
    pub available_from: Option<String>,
    pub running: bool,
    pub stats: PlayStats,
    pub job: Option<Job>,
}

#[derive(Serialize)]
pub struct LauncherStatus {
    pub version: String,
    pub latest: Option<String>,
    pub update_available: bool,
    pub job: Option<Job>,
    pub platform: &'static str,
    pub data_dir: PathBuf,
    pub log_file: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct CacheInfo {
    pub installers: u64,
    pub catalog: u64,
    pub logs: u64,
    pub download_dir: PathBuf,
    pub free_download: Option<u64>,
    pub free_install: Option<u64>,
}

struct AppHooks {
    app: Mutex<Weak<App>>,
    handle: AppHandle,
}

impl Hooks for AppHooks {
    fn changed(&self, jobs: &[Job]) {
        let _ = self.handle.emit("jobs", jobs);
    }
    fn ready(&self, job: Job) {
        if let Some(app) = self.app.lock().unwrap().upgrade() {
            tauri::async_runtime::spawn(async move { app.after_download(job).await });
        }
    }
}

impl App {
    pub fn new(handle: AppHandle) -> Arc<App> {
        let local = handle.path().local_data_dir().unwrap_or_else(|_| std::env::temp_dir());
        let paths = Paths::new(local);
        logger::init(paths.logs.clone());
        logger::info(&format!("Mooncrest Launcher {} starting", env!("CARGO_PKG_VERSION")));

        let defaults = Settings { download_dir: paths.default_downloads.clone(), install_dir: paths.default_library.clone(), ..Settings::default() };
        let settings = Settings::load(&paths.settings, &defaults);
        let (manifest, source) = catalog::load_local(&paths.catalog_cache());
        let (feed, nsource) = catalog::load_local_news(&paths.news_cache());

        let client = reqwest::Client::builder()
            .user_agent(concat!("MooncrestLauncher/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .https_only(!cfg!(debug_assertions))
            .build()
            .expect("http client");

        let hooks = Arc::new(AppHooks { app: Mutex::new(Weak::new()), handle: handle.clone() });
        let engine = Engine::new(client.clone(), hooks.clone(), Some(paths.jobs.clone()));
        let app = Arc::new(App {
            library: Library::open(paths.library.clone()),
            settings: RwLock::new(settings),
            catalog: RwLock::new(CatalogState { manifest, source, fetched_at: None, error: None }),
            news: RwLock::new(NewsState { feed, source: nsource, fetched_at: None, error: None }),
            engine,
            client,
            running: Mutex::new(HashMap::new()),
            hidden_for_product: Mutex::new(false),
            handle,
            paths,
        });
        *hooks.app.lock().unwrap() = Arc::downgrade(&app);
        app
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn manifest(&self) -> Manifest {
        self.catalog.read().unwrap().manifest.clone()
    }

    pub fn product(&self, id: &str) -> AppResult<Product> {
        self.manifest().product(id).cloned()
    }

    fn lang(&self) -> String {
        self.settings.read().unwrap().language.clone()
    }

    pub fn emit_library(&self) {
        let _ = self.handle.emit("library", ());
    }

    // ------------------------------------------------------------ catalog

    pub async fn refresh_catalog(&self) -> CatalogState {
        let url = self.settings().catalog_url;
        if url.is_empty() {
            return self.catalog.read().unwrap().clone();
        }
        let result = async {
            let text = catalog::fetch_text(&self.client, &url).await?;
            let m = Manifest::parse(&text)?;
            store::write_json(&self.paths.catalog_cache(), &m)?;
            Ok::<Manifest, AppError>(m)
        }
        .await;
        {
            let mut c = self.catalog.write().unwrap();
            match result {
                Ok(m) => {
                    *c = CatalogState { manifest: m, source: Source::Online, fetched_at: Some(store::now_ms()), error: None };
                }
                Err(e) => {
                    logger::warn(&format!("catalog refresh from {url} failed: {e}"));
                    c.error = Some(ErrorInfo::from(&e));
                    c.fetched_at = Some(store::now_ms());
                }
            }
        }
        let state = self.catalog.read().unwrap().clone();
        let _ = self.handle.emit("catalog", &state);
        state
    }

    pub async fn refresh_news(&self) -> NewsState {
        let Some(url) = catalog::news_url_for(&self.settings().catalog_url) else {
            return self.news.read().unwrap().clone();
        };
        let result = async {
            let text = catalog::fetch_text(&self.client, &url).await?;
            let f = NewsFeed::parse(&text)?;
            store::write_json(&self.paths.news_cache(), &f)?;
            Ok::<NewsFeed, AppError>(f)
        }
        .await;
        {
            let mut n = self.news.write().unwrap();
            match result {
                Ok(f) => *n = NewsState { feed: f, source: Source::Online, fetched_at: Some(store::now_ms()), error: None },
                Err(e) => {
                    n.error = Some(ErrorInfo::from(&e));
                    n.fetched_at = Some(store::now_ms());
                }
            }
        }
        let state = self.news.read().unwrap().clone();
        let _ = self.handle.emit("news", &state);
        state
    }

    // ------------------------------------------------------------ library

    pub fn is_running(&self, product: &Product, rec: Option<&InstallRecord>) -> bool {
        if self.running.lock().unwrap().contains_key(&product.id) {
            return true;
        }
        rec.and_then(|r| library::main_exe(product, r)).map(|exe| platform::exe_in_use(&exe)).unwrap_or(false)
    }

    pub fn library_items(&self) -> Vec<LibraryItem> {
        let manifest = self.manifest();
        manifest
            .products
            .iter()
            .map(|p| {
                let install = self.library.scan(p);
                let latest = p.latest();
                let update_available = match (&install, latest) {
                    (Some(rec), Some(l)) => rec.version.as_deref().map(|v| catalog::is_newer(&l.version, v)).unwrap_or(false),
                    _ => false,
                };
                LibraryItem {
                    product_id: p.id.clone(),
                    running: self.is_running(p, install.as_ref()),
                    latest: latest.map(|r| r.version.clone()),
                    available: latest.is_some(),
                    available_from: latest.and_then(|r| r.available_from.clone()),
                    update_available,
                    stats: self.library.stats(&p.id),
                    job: self.engine.active_for(&p.id),
                    install,
                }
            })
            .collect()
    }

    pub fn default_install_dir(&self, product: &Product) -> PathBuf {
        self.settings().install_dir.join(product.install.folder())
    }

    // ------------------------------------------------------------ install flow

    /// Queues a download + install (or update) of the newest release.
    pub fn start_install(&self, product_id: &str, install_dir: Option<PathBuf>) -> AppResult<Job> {
        let product = self.product(product_id)?;
        let release = product.latest().ok_or(AppError::Unsupported)?.clone();
        if let Some(from) = &release.available_from {
            if !available_now(from) {
                return Err(AppError::NotAvailable(format!("available from {from}")));
            }
        }
        let file = release.file_for_platform().ok_or(AppError::Unsupported)?.clone();
        let existing = self.library.scan(&product);
        let kind = if existing.is_some() { JobKind::Update } else { JobKind::Install };
        let target = match (&existing, install_dir) {
            (Some(_), _) => None,
            (None, Some(d)) => Some(d),
            (None, None) => Some(self.default_install_dir(&product)),
        };
        if let Some(t) = &target {
            check_target(t)?;
        }
        self.engine.enqueue(NewJob {
            product_id: product.id.clone(),
            product_name: product.name.clone(),
            version: release.version.clone(),
            kind,
            file_name: file.file_name.clone(),
            url: file.url.clone(),
            sha256: file.sha256.clone(),
            size: file.size,
            dest_dir: self.settings().download_dir.clone(),
            local_source: None,
            install_dir: target,
        })
    }

    /// Install from a setup file the user already has. The file must match a
    /// release of this product in the catalog (SHA-256), otherwise it is refused.
    pub fn install_from_file(&self, product_id: &str, file: PathBuf, install_dir: Option<PathBuf>) -> AppResult<Job> {
        let product = self.product(product_id)?;
        let meta = std::fs::metadata(&file).map_err(|e| AppError::Invalid(format!("{}: {e}", file.display())))?;
        let candidates: Vec<_> = product
            .releases
            .iter()
            .filter_map(|r| r.file_for_platform().map(|f| (r, f)))
            .filter(|(_, f)| f.size == meta.len())
            .collect();
        if candidates.is_empty() {
            return Err(AppError::Invalid("this file does not match any release of this product (size differs)".into()));
        }
        let hash = crate::download::sha256_file(&file)?;
        let (release, rf) = candidates
            .into_iter()
            .find(|(_, f)| f.sha256.eq_ignore_ascii_case(&hash))
            .ok_or_else(|| AppError::Invalid("this file does not match any release of this product (checksum differs)".into()))?;
        let existing = self.library.scan(&product);
        if let Some(rec) = &existing {
            if let Some(v) = &rec.version {
                if catalog::compare_versions(&release.version, v) == std::cmp::Ordering::Less {
                    return Err(AppError::Invalid(format!("version {} is older than the installed {v}", release.version)));
                }
            }
        }
        let kind = if existing.is_some() { JobKind::Update } else { JobKind::Install };
        let target = if existing.is_some() { None } else { Some(install_dir.unwrap_or_else(|| self.default_install_dir(&product))) };
        if let Some(t) = &target {
            check_target(t)?;
        }
        self.engine.enqueue(NewJob {
            product_id: product.id.clone(),
            product_name: product.name.clone(),
            version: release.version.clone(),
            kind,
            file_name: rf.file_name.clone(),
            url: rf.url.clone(),
            sha256: rf.sha256.clone(),
            size: rf.size,
            dest_dir: self.settings().download_dir.clone(),
            local_source: Some(file),
            install_dir: target,
        })
    }

    /// Called by the engine when a file is downloaded and verified.
    pub async fn after_download(self: Arc<Self>, job: Job) {
        if job.kind == JobKind::Launcher {
            let _ = self.handle.emit("launcher-update-ready", &job);
            self.notify_text("launcher_update_ready", None);
            return;
        }
        let Ok(product) = self.product(&job.product_id) else {
            let _ = self.engine.update(&job.id, |j| {
                j.state = JobState::Failed;
                j.error = Some(ErrorInfo { code: "invalid".into(), message: "product no longer in the catalog".into() });
                Ok(())
            });
            return;
        };
        let existing = self.library.scan(&product);
        if self.is_running(&product, existing.as_ref()) {
            let _ = self.engine.update(&job.id, |j| {
                j.state = JobState::WaitingForExit;
                Ok(())
            });
            return;
        }
        let _ = self.engine.update(&job.id, |j| {
            j.state = JobState::Installing;
            j.note = None;
            Ok(())
        });
        let me = self.clone();
        let j = job.clone();
        let res = tauri::async_runtime::spawn_blocking(move || {
            install::install(InstallRequest {
                product: &product,
                payload: j.payload(),
                version: &j.version,
                target: j.install_dir.clone(),
                existing,
                is_update: j.kind == JobKind::Update,
            })
        })
        .await
        .map_err(|e| AppError::InstallFailed(e.to_string()))
        .and_then(|r| r);
        match res {
            Ok(rec) => {
                logger::info(&format!("{} {} installed at {}", job.product_id, job.version, rec.path.display()));
                me.library.put(rec);
                let _ = me.engine.update(&job.id, |j| {
                    j.state = JobState::Completed;
                    j.error = None;
                    Ok(())
                });
                if job.local_source.is_none() && !me.settings().keep_installers {
                    let _ = std::fs::remove_file(&job.dest);
                }
                me.notify_text(if job.kind == JobKind::Update { "updated" } else { "installed" }, Some(&job.product_name));
            }
            Err(e) => {
                logger::error(&format!("install of {} failed: {e}", job.product_id));
                let _ = me.engine.update(&job.id, |j| {
                    j.state = JobState::Failed;
                    j.error = Some(ErrorInfo::from(&e));
                    Ok(())
                });
                me.notify_text("install_failed", Some(&job.product_name));
            }
        }
        me.emit_library();
    }

    /// "Retry" on a failed job: re-verifies the downloaded file and installs again.
    pub fn retry(&self, id: &str) -> AppResult<()> {
        self.engine.resume(id)
    }

    pub async fn uninstall(self: Arc<Self>, product_id: &str) -> AppResult<()> {
        let product = self.product(product_id)?;
        let rec = self.library.scan(&product).ok_or(AppError::NotInstalled)?;
        if self.engine.active_for(product_id).is_some() {
            return Err(AppError::Busy);
        }
        let p = product.clone();
        let r = rec.clone();
        tauri::async_runtime::spawn_blocking(move || install::uninstall(&p, &r))
            .await
            .map_err(|e| AppError::InstallFailed(e.to_string()))??;
        self.library.remove(product_id);
        self.library.scan(&product);
        logger::info(&format!("{product_id} uninstalled"));
        self.emit_library();
        Ok(())
    }

    pub fn locate(&self, product_id: &str, dir: PathBuf) -> AppResult<InstallRecord> {
        let product = self.product(product_id)?;
        let rec = library::locate(&product, &dir)?;
        self.library.put(rec.clone());
        self.emit_library();
        Ok(rec)
    }

    // ------------------------------------------------------------ launching

    pub fn launch(self: &Arc<Self>, product_id: &str) -> AppResult<()> {
        let product = self.product(product_id)?;
        let rec = self.library.scan(&product).ok_or(AppError::NotInstalled)?;
        let exe = library::main_exe(&product, &rec).ok_or(AppError::Unsupported)?;
        if !exe.is_file() {
            return Err(AppError::NotInstalled);
        }
        if self.is_running(&product, Some(&rec)) {
            return Err(AppError::Running);
        }
        if let Some(job) = self.engine.active_for(product_id) {
            if matches!(job.state, JobState::Installing) {
                return Err(AppError::Busy);
            }
        }
        let mut child = std::process::Command::new(&exe)
            .current_dir(&rec.path)
            .spawn()
            .map_err(|e| AppError::Io(format!("could not start {}: {e}", exe.display())))?;
        logger::info(&format!("launched {} ({})", product_id, exe.display()));
        self.library.record_launch(product_id);
        self.running.lock().unwrap().insert(product_id.to_string(), Running { started: Instant::now() });
        self.emit_library();

        if let Some(win) = self.handle.get_webview_window("main") {
            match self.settings().on_product_launch {
                LaunchBehavior::Keep => {}
                LaunchBehavior::Minimize => {
                    let _ = win.minimize();
                    *self.hidden_for_product.lock().unwrap() = true;
                }
                LaunchBehavior::Tray => {
                    let _ = win.hide();
                    *self.hidden_for_product.lock().unwrap() = true;
                }
            }
        }

        let me = self.clone();
        let id = product_id.to_string();
        std::thread::spawn(move || {
            let _ = child.wait();
            let started = me.running.lock().unwrap().remove(&id).map(|r| r.started);
            if let Some(s) = started {
                me.library.add_playtime(&id, s.elapsed().as_secs());
            }
            logger::info(&format!("{id} exited"));
            if std::mem::take(&mut *me.hidden_for_product.lock().unwrap()) {
                if let Some(win) = me.handle.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.unminimize();
                }
            }
            me.emit_library();
            // An update that waited for the product to close can go in now.
            let waiting: Vec<Job> = me
                .engine
                .snapshot()
                .into_iter()
                .filter(|j| j.product_id == id && j.state == JobState::WaitingForExit)
                .collect();
            for j in waiting {
                let m = me.clone();
                tauri::async_runtime::spawn(async move { m.after_download(j).await });
            }
        });
        Ok(())
    }

    // ------------------------------------------------------------ updates

    /// Compares installed versions with the catalog and queues updates if allowed.
    pub fn check_product_updates(&self) -> Vec<String> {
        let settings = self.settings();
        let mut found = vec![];
        for item in self.library_items() {
            if !item.update_available {
                continue;
            }
            found.push(item.product_id.clone());
            if settings.auto_update_products && item.job.is_none() {
                match self.start_install(&item.product_id, None) {
                    Ok(_) => logger::info(&format!("auto-update queued for {}", item.product_id)),
                    Err(e) => logger::warn(&format!("auto-update for {} not queued: {e}", item.product_id)),
                }
            }
        }
        if !found.is_empty() {
            let names: Vec<String> = found.iter().filter_map(|id| self.product(id).ok().map(|p| p.name)).collect();
            self.notify_text("update_available", Some(&names.join(", ")));
        }
        found
    }

    pub fn launcher_status(&self) -> LauncherStatus {
        let current = env!("CARGO_PKG_VERSION");
        let latest = self.manifest().launcher.map(|l| l.latest);
        let downloadable = latest.as_ref().and_then(|r| r.file_for_platform()).is_some();
        let update_available = downloadable && latest.as_ref().map(|r| catalog::is_newer(&r.version, current)).unwrap_or(false);
        LauncherStatus {
            version: current.to_string(),
            latest: latest.map(|r| r.version),
            update_available,
            job: self.engine.snapshot().into_iter().rev().find(|j| j.kind == JobKind::Launcher && j.state != JobState::Cancelled),
            platform: catalog::PLATFORM,
            data_dir: self.paths.base.clone(),
            log_file: logger::path(),
        }
    }

    pub fn download_launcher_update(&self) -> AppResult<Job> {
        let status = self.launcher_status();
        if !status.update_available {
            return Err(AppError::Invalid("no launcher update available".into()));
        }
        let release = self.manifest().launcher.ok_or(AppError::Unsupported)?.latest;
        let file = release.file_for_platform().ok_or(AppError::Unsupported)?.clone();
        self.engine.enqueue(NewJob {
            product_id: LAUNCHER_PRODUCT_ID.into(),
            product_name: "Mooncrest Launcher".into(),
            version: release.version.clone(),
            kind: JobKind::Launcher,
            file_name: file.file_name,
            url: file.url,
            sha256: file.sha256,
            size: file.size,
            dest_dir: self.settings().download_dir.clone(),
            local_source: None,
            install_dir: None,
        })
    }

    /// Runs the verified launcher setup in update mode and quits; the setup
    /// starts the new version when it is done (`/R`).
    pub fn apply_launcher_update(&self) -> AppResult<()> {
        let job = self
            .engine
            .snapshot()
            .into_iter()
            .rev()
            .find(|j| j.kind == JobKind::Launcher && j.state == JobState::Ready)
            .ok_or_else(|| AppError::Invalid("the update is not downloaded yet".into()))?;
        // Check the file once more right before running it.
        if crate::download::sha256_file(&job.dest)? != job.sha256 {
            return Err(AppError::VerifyFailed { expected: job.sha256.clone(), actual: "changed on disk".into() });
        }
        if !cfg!(windows) {
            return Err(AppError::Unsupported);
        }
        let _ = self.engine.update(&job.id, |j| {
            j.state = JobState::Completed;
            Ok(())
        });
        platform::command(&job.dest)
            .args(["/S", "/UPDATE", "/R"])
            .spawn()
            .map_err(|e| AppError::InstallFailed(format!("could not start the launcher setup: {e}")))?;
        logger::info("launcher update started, exiting");
        self.handle.exit(0);
        Ok(())
    }

    /// Background loop: refresh catalog and news, scan, check updates. Every 6 hours.
    pub async fn background(self: Arc<Self>) {
        loop {
            self.refresh_catalog().await;
            self.refresh_news().await;
            self.emit_library();
            self.check_product_updates();
            if self.settings().auto_update_launcher && self.launcher_status().update_available && self.engine.active_for(LAUNCHER_PRODUCT_ID).is_none() {
                let st = self.launcher_status();
                let already = st.job.map(|j| j.version == st.latest.clone().unwrap_or_default() && j.state.is_active()).unwrap_or(false);
                if !already {
                    if let Err(e) = self.download_launcher_update() {
                        logger::warn(&format!("launcher update not queued: {e}"));
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    }

    // ------------------------------------------------------------ cache

    pub fn cache_info(&self) -> CacheInfo {
        let s = self.settings();
        CacheInfo {
            installers: store::dir_size(&s.download_dir),
            catalog: store::dir_size(&self.paths.cache),
            logs: store::dir_size(&self.paths.logs),
            free_download: store::free_space(&s.download_dir),
            free_install: store::free_space(&s.install_dir),
            download_dir: s.download_dir,
        }
    }

    pub fn clear_cache(&self, kind: &str) -> AppResult<CacheInfo> {
        match kind {
            "installers" => {
                // Never touch files of jobs that are still in progress.
                let busy: Vec<PathBuf> = self
                    .engine
                    .snapshot()
                    .into_iter()
                    .filter(|j| j.state.is_active())
                    .flat_map(|j| [j.dest.clone(), j.part_path()])
                    .collect();
                let dir = self.settings().download_dir;
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.is_file() && !busy.contains(&p) {
                            let _ = std::fs::remove_file(&p);
                        }
                    }
                }
            }
            "catalog" => {
                let _ = std::fs::remove_dir_all(&self.paths.cache);
            }
            "logs" => {
                if let Ok(rd) = std::fs::read_dir(&self.paths.logs) {
                    for e in rd.flatten() {
                        let _ = std::fs::write(e.path(), b"");
                    }
                }
            }
            _ => return Err(AppError::Invalid("unknown cache".into())),
        }
        Ok(self.cache_info())
    }

    // ------------------------------------------------------------ profile

    pub fn profile(&self) -> Profile {
        store::read_json(&self.paths.profile).ok().flatten().unwrap_or_default()
    }

    pub fn save_profile(&self, mut p: Profile) -> AppResult<Profile> {
        p.display_name = p.display_name.trim().chars().take(32).collect();
        if p.created_at == 0 {
            let old = self.profile().created_at;
            p.created_at = if old > 0 { old } else { store::now_ms() };
        }
        store::write_json(&self.paths.profile, &p)?;
        Ok(p)
    }

    // ------------------------------------------------------------ notifications

    fn notify_text(&self, key: &str, name: Option<&str>) {
        let s = self.settings();
        if !s.notifications {
            return;
        }
        let wants = match key {
            "update_available" | "launcher_update_ready" => s.notify_updates,
            _ => s.notify_downloads,
        };
        if !wants {
            return;
        }
        let n = name.unwrap_or("");
        let de = self.lang() == "de";
        let (title, body) = match (key, de) {
            ("installed", true) => (format!("{n} ist installiert"), "Bereit zum Starten.".to_string()),
            ("installed", false) => (format!("{n} is installed"), "Ready to play.".to_string()),
            ("updated", true) => (format!("{n} ist aktuell"), "Das Update wurde installiert.".to_string()),
            ("updated", false) => (format!("{n} is up to date"), "The update was installed.".to_string()),
            ("install_failed", true) => (format!("{n} konnte nicht installiert werden"), "Details stehen unter Downloads.".to_string()),
            ("install_failed", false) => (format!("{n} could not be installed"), "See Downloads for details.".to_string()),
            ("update_available", true) => ("Update verfügbar".to_string(), n.to_string()),
            ("update_available", false) => ("Update available".to_string(), n.to_string()),
            ("launcher_update_ready", true) => ("Launcher-Update bereit".to_string(), "Neu starten, um es zu installieren.".to_string()),
            ("launcher_update_ready", false) => ("Launcher update ready".to_string(), "Restart to install it.".to_string()),
            _ => return,
        };
        use tauri_plugin_notification::NotificationExt;
        let _ = self.handle.notification().builder().title(title).body(body).show();
    }
}

fn check_target(dir: &Path) -> AppResult<()> {
    if !dir.is_absolute() {
        return Err(AppError::Invalid("install folder must be an absolute path".into()));
    }
    let parent = dir.parent().unwrap_or(dir);
    if !store::is_writable(parent) {
        return Err(AppError::Invalid(format!("no write access to {}", parent.display())));
    }
    if dir.exists() && dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        return Err(AppError::Invalid(format!("{} is not empty", dir.display())));
    }
    Ok(())
}

/// `available_from` is RFC 3339 like `2026-11-22T16:00:00+01:00`.
fn available_now(from: &str) -> bool {
    parse_rfc3339_ms(from).map(|t| store::now_ms() >= t).unwrap_or(true)
}

fn parse_rfc3339_ms(s: &str) -> Option<u64> {
    // Minimal parser for "YYYY-MM-DDTHH:MM:SS(+HH:MM|Z)".
    let (date, rest) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let (time, offset) = if let Some(t) = rest.strip_suffix('Z') {
        (t, 0)
    } else {
        let idx = rest.rfind(['+', '-'])?;
        let (t, off) = rest.split_at(idx);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let mut o = off[1..].split(':').map(|x| x.parse::<i64>());
        (t, sign * (o.next()?.ok()? * 3600 + o.next().and_then(|r| r.ok()).unwrap_or(0) * 60))
    };
    let mut t = time.split(':').map(|x| x.parse::<f64>());
    let (hh, mm, ss) = (t.next()?.ok()? as i64, t.next()?.ok()? as i64, t.next().and_then(|r| r.ok()).unwrap_or(0.0) as i64);
    // Days from civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let secs = days * 86400 + hh * 3600 + mm * 60 + ss - offset;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rfc3339() {
        // 2026-11-22T16:00:00+01:00 == 2026-11-22T15:00:00Z == 1795359600
        assert_eq!(super::parse_rfc3339_ms("2026-11-22T16:00:00+01:00"), Some(1_795_359_600_000));
        assert_eq!(super::parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
    }
}
