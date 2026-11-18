//! Mooncrest Launcher – Tauri entry point and IPC commands.

mod app;
mod catalog;
mod download;
mod error;
mod install;
mod library;
mod logger;
mod platform;
mod settings;
mod store;

pub use download::{Engine, Hooks, Job, JobKind, JobState, NewJob};
pub use error::{AppError, AppResult};

use app::{App, CacheInfo, CatalogState, LauncherStatus, LibraryItem, NewsState, Profile};
use library::InstallRecord;
use settings::Settings;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, State, WindowEvent};

type S<'a> = State<'a, Arc<App>>;

#[tauri::command]
fn get_settings(app: S) -> Settings {
    app.settings()
}

#[tauri::command]
fn save_settings(app: S, settings: Settings) -> AppResult<Settings> {
    settings.validate()?;
    std::fs::create_dir_all(&settings.download_dir).map_err(|e| AppError::Invalid(format!("download folder: {e}")))?;
    if !store::is_writable(&settings.download_dir) {
        return Err(AppError::Invalid("no write access to the download folder".into()));
    }
    std::fs::create_dir_all(&settings.install_dir).map_err(|e| AppError::Invalid(format!("install folder: {e}")))?;
    let old = app.settings();
    if old.launch_on_startup != settings.launch_on_startup {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.handle.autolaunch();
        let r = if settings.launch_on_startup { al.enable() } else { al.disable() };
        r.map_err(|e| AppError::Io(format!("autostart: {e}")))?;
    }
    store::write_json(&app.paths.settings, &settings)?;
    *app.settings.write().unwrap() = settings.clone();
    if old.catalog_url != settings.catalog_url {
        let a = app.inner().clone();
        tauri::async_runtime::spawn(async move {
            a.refresh_catalog().await;
            a.refresh_news().await;
            a.emit_library();
        });
    }
    Ok(settings)
}

#[tauri::command]
fn reset_settings(app: S) -> AppResult<Settings> {
    let defaults = Settings {
        download_dir: app.paths.default_downloads.clone(),
        install_dir: app.paths.default_library.clone(),
        ..Settings::default()
    };
    save_settings(app, defaults)
}

#[tauri::command]
fn get_catalog(app: S) -> CatalogState {
    app.catalog.read().unwrap().clone()
}

#[tauri::command]
async fn refresh_catalog(app: S<'_>) -> AppResult<CatalogState> {
    let a = app.inner().clone();
    let state = a.refresh_catalog().await;
    a.emit_library();
    a.check_product_updates();
    Ok(state)
}

#[tauri::command]
fn get_news(app: S) -> NewsState {
    app.news.read().unwrap().clone()
}

#[tauri::command]
async fn refresh_news(app: S<'_>) -> AppResult<NewsState> {
    Ok(app.inner().clone().refresh_news().await)
}

#[tauri::command]
async fn get_library(app: S<'_>) -> AppResult<Vec<LibraryItem>> {
    let a = app.inner().clone();
    // Scanning touches the registry and disk: keep it off the UI thread.
    tauri::async_runtime::spawn_blocking(move || a.library_items()).await.map_err(|e| AppError::Io(e.to_string()))
}

#[tauri::command]
fn default_install_dir(app: S, product_id: String) -> AppResult<PathBuf> {
    Ok(app.default_install_dir(&app.product(&product_id)?))
}

#[tauri::command]
fn start_install(app: S, product_id: String, install_dir: Option<PathBuf>) -> AppResult<Job> {
    app.start_install(&product_id, install_dir)
}

#[tauri::command]
async fn install_from_file(app: S<'_>, product_id: String, file: PathBuf, install_dir: Option<PathBuf>) -> AppResult<Job> {
    let a = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || a.install_from_file(&product_id, file, install_dir))
        .await
        .map_err(|e| AppError::Io(e.to_string()))?
}

#[tauri::command]
fn locate_install(app: S, product_id: String, dir: PathBuf) -> AppResult<InstallRecord> {
    app.locate(&product_id, dir)
}

#[tauri::command]
async fn uninstall_product(app: S<'_>, product_id: String) -> AppResult<()> {
    app.inner().clone().uninstall(&product_id).await
}

#[tauri::command]
fn launch_product(app: S, product_id: String) -> AppResult<()> {
    app.inner().launch(&product_id)
}

#[tauri::command]
fn get_jobs(app: S) -> Vec<Job> {
    app.engine.snapshot()
}

#[tauri::command]
fn pause_job(app: S, id: String) -> AppResult<()> {
    app.engine.pause(&id)
}

#[tauri::command]
fn resume_job(app: S, id: String) -> AppResult<()> {
    app.engine.resume(&id)
}

#[tauri::command]
fn cancel_job(app: S, id: String) -> AppResult<()> {
    app.engine.cancel(&id)
}

#[tauri::command]
fn retry_job(app: S, id: String) -> AppResult<()> {
    app.retry(&id)
}

#[tauri::command]
fn remove_job(app: S, id: String) -> AppResult<()> {
    app.engine.remove(&id)
}

#[tauri::command]
fn clear_job_history(app: S) {
    app.engine.clear_history()
}

#[tauri::command]
fn launcher_status(app: S) -> LauncherStatus {
    app.launcher_status()
}

#[tauri::command]
fn download_launcher_update(app: S) -> AppResult<Job> {
    app.download_launcher_update()
}

#[tauri::command]
fn apply_launcher_update(app: S) -> AppResult<()> {
    app.apply_launcher_update()
}

#[tauri::command]
fn cache_info(app: S) -> CacheInfo {
    app.cache_info()
}

#[tauri::command]
fn clear_cache(app: S, kind: String) -> AppResult<CacheInfo> {
    app.clear_cache(&kind)
}

#[tauri::command]
fn get_profile(app: S) -> Profile {
    app.profile()
}

#[tauri::command]
fn save_profile(app: S, profile: Profile) -> AppResult<Profile> {
    app.save_profile(profile)
}

#[tauri::command]
fn reset_stats(app: S) {
    // Clears play statistics only; installs are re-detected right after.
    app.library.reset();
    app.emit_library();
}

#[tauri::command]
fn open_path(app: S, path: PathBuf) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    // Only folders the launcher manages or products live in.
    let s = app.settings();
    let mut allowed = vec![s.download_dir.clone(), s.install_dir.clone(), app.paths.base.clone()];
    for item in app.library_items() {
        if let Some(r) = item.install {
            allowed.push(r.path);
        }
    }
    let ok = allowed.iter().any(|a| path.starts_with(a));
    if !ok {
        return Err(AppError::Invalid("this folder is not managed by the launcher".into()));
    }
    let target = if path.is_file() { path.parent().map(PathBuf::from).unwrap_or(path) } else { path };
    std::fs::create_dir_all(&target).ok();
    app.handle.opener().open_path(target.to_string_lossy().to_string(), None::<&str>).map_err(|e| AppError::Io(e.to_string()))
}

#[tauri::command]
fn open_url(app: S, url: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with("https://") {
        return Err(AppError::Invalid("only https links can be opened".into()));
    }
    app.handle.opener().open_url(url, None::<&str>).map_err(|e| AppError::Io(e.to_string()))
}

#[tauri::command]
fn free_space(path: PathBuf) -> Option<u64> {
    store::free_space(&path)
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn run() {
    let started_minimized = std::env::args().any(|a| a == "--minimized");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(move |tauri_app| {
            let app = App::new(tauri_app.handle().clone());
            tauri_app.manage(app.clone());

            let open = MenuItem::with_id(tauri_app, "open", "Mooncrest Launcher", true, None::<&str>)?;
            let quit = MenuItem::with_id(tauri_app, "quit", "Quit / Beenden", true, None::<&str>)?;
            let menu = Menu::with_items(tauri_app, &[&open, &quit])?;
            TrayIconBuilder::with_id("tray")
                .icon(tauri_app.default_window_icon().cloned().expect("icon"))
                .tooltip("Mooncrest Launcher")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_main(tray.app_handle());
                    }
                })
                .build(tauri_app)?;

            let hidden = started_minimized || app.settings().start_minimized;
            if !hidden {
                show_main(tauri_app.handle());
            }

            app.engine.start();
            tauri::async_runtime::spawn(app.clone().background());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().state::<Arc<App>>();
                if app.settings().close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            reset_settings,
            get_catalog,
            refresh_catalog,
            get_news,
            refresh_news,
            get_library,
            default_install_dir,
            start_install,
            install_from_file,
            locate_install,
            uninstall_product,
            launch_product,
            get_jobs,
            pause_job,
            resume_job,
            cancel_job,
            retry_job,
            remove_job,
            clear_job_history,
            launcher_status,
            download_launcher_update,
            apply_launcher_update,
            cache_info,
            clear_cache,
            get_profile,
            save_profile,
            reset_stats,
            open_path,
            open_url,
            free_space
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mooncrest Launcher");
}
