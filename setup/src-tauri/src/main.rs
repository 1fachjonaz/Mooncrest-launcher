//! Mooncrest Launcher Setup – the launcher's own installer and uninstaller.
//!
//! One .exe carries the launcher files (payload.zip, compiled in).
//!   Mooncrest-Launcher-Setup.exe                 install / update with the setup window
//!   Mooncrest-Launcher-Setup.exe /S [/UPDATE] [/R] [/D=C:\path]
//!                                                silent install or update (used by the launcher's self-update),
//!                                                /R starts the launcher afterwards
//!   uninstall.exe --uninstall [/S]               uninstall (copied into the install folder)
//!
//! Per-user install, no administrator rights: files in %LOCALAPPDATA%\Programs\Mooncrest Launcher,
//! uninstall entry in HKCU, shortcuts in the user's Start menu / desktop.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const PRODUCT: &str = "Mooncrest Launcher";
#[allow(dead_code)]
const PUBLISHER: &str = "Mooncrest Group";
const EXE: &str = "mooncrest-launcher.exe";
const UNINSTALLER: &str = "uninstall.exe";
const VERSION: &str = env!("CARGO_PKG_VERSION");
#[allow(dead_code)]
const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mooncrest Launcher";
const PAYLOAD: &[u8] = include_bytes!("../../payload/payload.zip");

#[derive(Default, Debug)]
struct Args {
    silent: bool,
    update: bool,
    relaunch: bool,
    uninstall: bool,
    dir: Option<PathBuf>,
}

fn parse_args() -> Args {
    parse_from(&std::env::args().skip(1).collect::<Vec<_>>())
}

fn parse_from(raw: &[String]) -> Args {
    let mut a = Args::default();
    let mut i = 0;
    while i < raw.len() {
        let s = raw[i].as_str();
        match s.to_ascii_uppercase().as_str() {
            "/S" | "--SILENT" => a.silent = true,
            "/UPDATE" | "--UPDATE" => a.update = true,
            "/R" | "--RUN" => a.relaunch = true,
            "--UNINSTALL" | "/UNINSTALL" => a.uninstall = true,
            _ if s.len() > 3 && s[..3].eq_ignore_ascii_case("/D=") => {
                // NSIS convention: /D= is last and unquoted, so it may have been split at spaces.
                let mut p = s[3..].to_string();
                for rest in &raw[i + 1..] {
                    p.push(' ');
                    p.push_str(rest);
                }
                a.dir = Some(PathBuf::from(p.trim_matches('"')));
                break;
            }
            _ => {}
        }
        i += 1;
    }
    a
}

// ------------------------------------------------------------------ system helpers

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

fn default_dir() -> PathBuf {
    local_app_data().join("Programs").join(PRODUCT)
}

fn hidden_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

#[derive(Serialize, Clone)]
struct Existing {
    dir: PathBuf,
    version: Option<String>,
}

#[cfg(windows)]
fn existing() -> Option<Existing> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY).ok()?;
    let dir: String = key.get_value("InstallLocation").ok()?;
    let dir = PathBuf::from(dir.trim().trim_matches('"'));
    if !dir.join(EXE).is_file() {
        return None;
    }
    Some(Existing { dir, version: key.get_value("DisplayVersion").ok() })
}

#[cfg(not(windows))]
fn existing() -> Option<Existing> {
    None
}

fn free_space(path: &Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            return None;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut avail = 0u64;
        let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, std::ptr::null_mut(), std::ptr::null_mut()) };
        if ok != 0 { Some(avail) } else { None }
    }
    #[cfg(not(windows))]
    {
        let _ = p;
        None
    }
}

fn payload_size() -> u64 {
    let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(PAYLOAD)) else { return 0 };
    (0..z.len()).filter_map(|i| z.by_index(i).ok().map(|f| f.size())).sum()
}

/// Windows keeps a running .exe locked; wait until the launcher has closed.
fn wait_unlocked(exe: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    loop {
        if !exe.exists() || fs::OpenOptions::new().write(true).open(exe).is_ok() {
            return true;
        }
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn start_menu_link() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Microsoft\\Windows\\Start Menu\\Programs").join(format!("{PRODUCT}.lnk")))
}

fn desktop_link() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Desktop").join(format!("{PRODUCT}.lnk")))
}

fn create_shortcut(link: &Path, target: &Path, dir: &Path) -> Result<(), String> {
    if let Some(p) = link.parent() {
        let _ = fs::create_dir_all(p);
    }
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut({});$s.TargetPath={};$s.WorkingDirectory={};$s.IconLocation={};$s.Description={};$s.Save()",
        ps_quote(&link.to_string_lossy()),
        ps_quote(&target.to_string_lossy()),
        ps_quote(&dir.to_string_lossy()),
        ps_quote(&format!("{},0", target.to_string_lossy())),
        ps_quote("Mooncrest Launcher – Beyond the Horizon."),
    );
    let out = hidden_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .output()
        .map_err(|e| format!("shortcut: {e}"))?;
    if !out.status.success() {
        return Err(format!("shortcut: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

#[cfg(windows)]
fn write_registry(dir: &Path, size_bytes: u64) -> Result<(), String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY).map_err(|e| format!("registry: {e}"))?;
    let d = dir.to_string_lossy().to_string();
    let uninst = dir.join(UNINSTALLER).to_string_lossy().to_string();
    let set = |k: &str, v: &str| key.set_value(k, &v.to_string()).map_err(|e| format!("registry {k}: {e}"));
    set("DisplayName", PRODUCT)?;
    set("DisplayVersion", VERSION)?;
    set("Publisher", PUBLISHER)?;
    set("InstallLocation", &d)?;
    set("MainBinaryName", EXE)?;
    set("DisplayIcon", &dir.join(EXE).to_string_lossy())?;
    set("UninstallString", &format!("\"{uninst}\" --uninstall"))?;
    set("QuietUninstallString", &format!("\"{uninst}\" --uninstall /S"))?;
    set("URLInfoAbout", "https://mlbclient.vercel.app/")?;
    key.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
    key.set_value("EstimatedSize", &((size_bytes / 1024) as u32)).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
fn write_registry(_dir: &Path, _size: u64) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn remove_registry() {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(UNINSTALL_KEY);
    // Autostart entry written by the launcher's "Mit Windows starten" setting.
    if let Ok(run) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", winreg::enums::KEY_ALL_ACCESS) {
        let _ = run.delete_value(PRODUCT);
    }
}

#[cfg(not(windows))]
fn remove_registry() {}

// ------------------------------------------------------------------ install / uninstall

#[derive(Clone, Serialize)]
struct Progress {
    stage: &'static str,
    pct: f64,
}

#[derive(Clone, Copy)]
struct Options {
    desktop: bool,
    start_menu: bool,
}

fn install(dir: &Path, opts: Options, update: bool, progress: &dyn Fn(Progress)) -> Result<(), String> {
    progress(Progress { stage: "prepare", pct: 0.02 });
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let probe = dir.join(".mooncrest-write-test");
    fs::write(&probe, b"").map_err(|_| format!("no write access to {}", dir.display()))?;
    let _ = fs::remove_file(&probe);

    let total = payload_size().max(1);
    let needed = total + 32 * 1024 * 1024;
    if let Some(free) = free_space(dir) {
        if free < needed {
            return Err(format!("not enough disk space ({} MB needed)", needed / 1024 / 1024));
        }
    }

    progress(Progress { stage: "waiting", pct: 0.04 });
    if !wait_unlocked(&dir.join(EXE), Duration::from_secs(if update { 30 } else { 4 })) {
        return Err("running".into());
    }

    // Files are written next to their final name and swapped in at the end, so an
    // interrupted install never leaves a half-written launcher behind.
    progress(Progress { stage: "copy", pct: 0.06 });
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(PAYLOAD)).map_err(|e| format!("payload: {e}"))?;
    let mut written = 0u64;
    let mut staged: Vec<(PathBuf, PathBuf)> = vec![];
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| format!("payload: {e}"))?;
        let rel = f.enclosed_name().ok_or("payload: bad path")?;
        let out = dir.join(&rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let tmp = out.with_extension("mc-new");
        let mut w = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut w, &buf[..n]).map_err(|e| e.to_string())?;
            written += n as u64;
            progress(Progress { stage: "copy", pct: 0.06 + 0.74 * (written as f64 / total as f64) });
        }
        w.sync_all().map_err(|e| e.to_string())?;
        staged.push((tmp, out));
    }
    for (tmp, out) in &staged {
        let _ = fs::remove_file(out);
        fs::rename(tmp, out).map_err(|e| format!("{}: {e}", out.display()))?;
    }

    // The setup copies itself into the folder and serves as the uninstaller.
    progress(Progress { stage: "register", pct: 0.84 });
    if let Ok(me) = std::env::current_exe() {
        let target = dir.join(UNINSTALLER);
        if me != target {
            let _ = fs::remove_file(&target);
            fs::copy(&me, &target).map_err(|e| format!("uninstaller: {e}"))?;
        }
    }
    write_registry(dir, total)?;

    progress(Progress { stage: "shortcuts", pct: 0.92 });
    let exe = dir.join(EXE);
    if opts.start_menu {
        if let Some(l) = start_menu_link() {
            create_shortcut(&l, &exe, dir)?;
        }
    }
    if opts.desktop {
        if let Some(l) = desktop_link() {
            create_shortcut(&l, &exe, dir)?;
        }
    }
    progress(Progress { stage: "done", pct: 1.0 });
    Ok(())
}

fn uninstall(dir: &Path, remove_data: bool) -> Result<(), String> {
    if !wait_unlocked(&dir.join(EXE), Duration::from_secs(3)) {
        return Err("running".into());
    }
    for link in [start_menu_link(), desktop_link()].into_iter().flatten() {
        let _ = fs::remove_file(link);
    }
    remove_registry();
    // Only files this setup (or the earlier NSIS setup) put there.
    for name in [EXE, "WebView2Loader.dll"] {
        let p = dir.join(name);
        if p.exists() {
            fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    if remove_data {
        let la = local_app_data();
        // Settings, statistics, queue, logs and the window's web data – never the products
        // in Mooncrest\Library or files in Mooncrest\Downloads.
        let _ = fs::remove_dir_all(la.join("Mooncrest").join("Launcher"));
        let _ = fs::remove_dir_all(la.join("group.mooncrest.launcher"));
    }
    // The uninstaller cannot delete itself while it runs: let cmd do it a moment later.
    let me = dir.join(UNINSTALLER);
    let cmd = format!(
        "ping 127.0.0.1 -n 3 > nul & del /f /q \"{}\" & rmdir \"{}\"",
        me.display(),
        dir.display()
    );
    let _ = hidden_command("cmd.exe").args(["/C", &cmd]).spawn();
    Ok(())
}

fn launch(dir: &Path) -> Result<(), String> {
    std::process::Command::new(dir.join(EXE))
        .current_dir(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ window commands

#[derive(Serialize)]
struct Info {
    version: &'static str,
    mode: &'static str,
    existing: Option<Existing>,
    default_dir: PathBuf,
    payload_size: u64,
}

struct Mode(bool);

#[tauri::command]
fn info(mode: tauri::State<Mode>) -> Info {
    let ex = existing();
    let m = if mode.0 {
        "uninstall"
    } else {
        match &ex {
            Some(e) if e.version.as_deref() == Some(VERSION) => "reinstall",
            Some(_) => "update",
            None => "install",
        }
    };
    Info {
        version: VERSION,
        mode: m,
        default_dir: ex.as_ref().map(|e| e.dir.clone()).unwrap_or_else(default_dir),
        existing: ex,
        payload_size: payload_size(),
    }
}

#[tauri::command]
fn disk_free(dir: PathBuf) -> Option<u64> {
    free_space(&dir)
}

#[tauri::command]
async fn run_install(app: tauri::AppHandle, dir: PathBuf, desktop: bool, start_menu: bool) -> Result<(), String> {
    if !dir.is_absolute() {
        return Err("invalid folder".into());
    }
    let update = existing().is_some();
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        install(&dir, Options { desktop, start_menu }, update, &|p| {
            let _ = h.emit("progress", p);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn close_launcher() -> Result<(), String> {
    hidden_command("taskkill.exe").args(["/IM", EXE, "/F"]).output().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(600));
    Ok(())
}

#[tauri::command]
async fn run_uninstall(remove_data: bool) -> Result<(), String> {
    let dir = existing()
        .map(|e| e.dir)
        .or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)))
        .ok_or("installation not found")?;
    tauri::async_runtime::spawn_blocking(move || uninstall(&dir, remove_data)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn launch_and_exit(app: tauri::AppHandle, dir: PathBuf) -> Result<(), String> {
    launch(&dir)?;
    app.exit(0);
    Ok(())
}

// ------------------------------------------------------------------ entry

fn main() {
    let args = parse_args();

    // Silent paths never open a window.
    if args.silent {
        let code = if args.uninstall {
            let dir = existing().map(|e| e.dir).unwrap_or_else(default_dir);
            match uninstall(&dir, false) {
                Ok(()) => 0,
                Err(_) => 1,
            }
        } else {
            let dir = args.dir.clone().or_else(|| existing().map(|e| e.dir)).unwrap_or_else(default_dir);
            let first = existing().is_none();
            let opts = Options { desktop: first, start_menu: true };
            match install(&dir, opts, args.update, &|_| {}) {
                Ok(()) => {
                    if args.relaunch {
                        let _ = launch(&dir);
                    }
                    0
                }
                Err(_) => 1,
            }
        };
        std::process::exit(code);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mode(args.uninstall))
        .setup(|app| {
            let title = if app.state::<Mode>().0 { "Mooncrest Launcher deinstallieren" } else { "Mooncrest Launcher Setup" };
            tauri::WebviewWindowBuilder::new(app, "setup", tauri::WebviewUrl::App("index.html".into()))
                .title(title)
                .inner_size(780.0, 500.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .center()
                .background_color(tauri::window::Color(16, 21, 34, 255))
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![info, disk_free, run_install, close_launcher, run_uninstall, launch_and_exit])
        .run(tauri::generate_context!())
        .expect("error while running the setup");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_and_uninstalls_into_folder() {
        let tmp = std::env::temp_dir().join(format!("mc-setup-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let dir = tmp.join("Mooncrest Launcher");
        let steps = std::sync::Mutex::new(vec![]);
        install(&dir, Options { desktop: false, start_menu: false }, false, &|p| steps.lock().unwrap().push(p.pct)).unwrap();
        assert!(dir.join(EXE).is_file(), "launcher exe installed");
        assert!(dir.join(UNINSTALLER).is_file(), "uninstaller copied");
        assert!(!fs::read_dir(&dir).unwrap().any(|e| e.unwrap().path().extension().is_some_and(|x| x == "mc-new")));
        let s = steps.lock().unwrap();
        assert!(s.windows(2).all(|w| w[1] >= w[0]) && *s.last().unwrap() == 1.0, "progress only goes up to 100 %");
        drop(s);
        // Reinstall over an existing installation works.
        install(&dir, Options { desktop: false, start_menu: false }, true, &|_| {}).unwrap();
        uninstall(&dir, false).unwrap();
        assert!(!dir.join(EXE).exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn parses_nsis_style_arguments() {
        let raw: Vec<String> = ["/S", "/UPDATE", "/R", "/D=C:\\Program", "Files\\Mooncrest", "Launcher"].iter().map(|s| s.to_string()).collect();
        let a = parse_from(&raw);
        assert!(a.silent && a.update && a.relaunch && !a.uninstall);
        assert_eq!(a.dir.unwrap(), PathBuf::from("C:\\Program Files\\Mooncrest Launcher"));
        let u = parse_from(&["--uninstall".to_string(), "/S".to_string()]);
        assert!(u.uninstall && u.silent);
    }
}
