//! Installer handlers, one per `InstallSpec` kind. All functions block and are
//! run on a blocking thread by the caller.

use crate::catalog::{InstallSpec, Product};
use crate::error::{AppError, AppResult};
use crate::library::{InstallRecord, InstallSource, Marker, MARKER};
use crate::{platform, store};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct InstallRequest<'a> {
    pub product: &'a Product,
    pub payload: &'a Path,
    pub version: &'a str,
    /// Target folder for a fresh install. `None` for updates (keep the current location).
    pub target: Option<PathBuf>,
    pub existing: Option<InstallRecord>,
    pub is_update: bool,
}

const INSTALLER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

pub fn install(req: InstallRequest) -> AppResult<InstallRecord> {
    let now = store::now_ms();
    match &req.product.install {
        InstallSpec::Nsis { registry_name, main_binary, silent_args, update_args, .. } => {
            if !cfg!(windows) {
                return Err(AppError::Unsupported);
            }
            if let Some(rec) = &req.existing {
                if platform::exe_in_use(&rec.path.join(main_binary)) {
                    return Err(AppError::Running);
                }
            }
            let mut cmd = platform::command(req.payload);
            let args = if req.is_update && req.existing.is_some() { update_args } else { silent_args };
            cmd.args(args);
            // An update keeps the current location; a fresh install goes where the user chose.
            let target = if req.existing.is_some() { None } else { req.target.clone() };
            if let Some(dir) = &target {
                if !store::is_writable(dir) {
                    return Err(AppError::InstallFailed(format!("no write access to {}", dir.display())));
                }
                platform::raw_arg(&mut cmd, &format!("/D={}", dir.display()));
            }
            crate::logger::info(&format!("running installer {} {:?} target={:?}", req.payload.display(), args, target));
            run_and_wait(cmd, "installer")?;

            // Trust what Windows reports after the install.
            let entry = platform::uninstall_entry(registry_name);
            let path = entry
                .as_ref()
                .and_then(|e| e.location.clone())
                .or(target)
                .or_else(|| req.existing.as_ref().map(|r| r.path.clone()))
                .ok_or_else(|| AppError::InstallFailed("installer finished but the installation was not found".into()))?;
            let bin = entry.as_ref().and_then(|e| e.main_binary.clone()).unwrap_or_else(|| main_binary.clone());
            if !path.join(&bin).is_file() {
                return Err(AppError::InstallFailed(format!("{} is missing after installation", bin)));
            }
            let version = entry.and_then(|e| e.version).unwrap_or_else(|| req.version.to_string());
            Ok(InstallRecord {
                product_id: req.product.id.clone(),
                version: Some(version),
                path,
                source: InstallSource::Managed,
                installed_at: req.existing.as_ref().map(|r| r.installed_at).unwrap_or(now),
                updated_at: now,
            })
        }
        InstallSpec::Zip { main_binary, strip_root, .. } => {
            let target = req
                .existing
                .as_ref()
                .map(|r| r.path.clone())
                .or(req.target.clone())
                .ok_or_else(|| AppError::Invalid("no install folder".into()))?;
            if platform::exe_in_use(&target.join(main_binary)) {
                return Err(AppError::Running);
            }
            extract_zip(req.payload, &target, *strip_root, &req.product.id, req.version)?;
            if !target.join(main_binary).is_file() {
                return Err(AppError::InstallFailed(format!("{main_binary} is missing in the archive")));
            }
            Ok(InstallRecord {
                product_id: req.product.id.clone(),
                version: Some(req.version.to_string()),
                path: target,
                source: InstallSource::Managed,
                installed_at: req.existing.as_ref().map(|r| r.installed_at).unwrap_or(now),
                updated_at: now,
            })
        }
        InstallSpec::File { .. } => {
            let dir = req
                .existing
                .as_ref()
                .and_then(|r| r.path.parent().map(Path::to_path_buf))
                .or(req.target.clone())
                .ok_or_else(|| AppError::Invalid("no install folder".into()))?;
            fs::create_dir_all(&dir)?;
            let name = req.payload.file_name().ok_or_else(|| AppError::Invalid("bad file".into()))?;
            let dest = dir.join(name);
            let tmp = dir.join(format!(".{}.mooncrest-tmp", name.to_string_lossy()));
            fs::copy(req.payload, &tmp)?;
            if let Some(old) = &req.existing {
                if old.path != dest {
                    let _ = fs::remove_file(&old.path);
                }
            }
            fs::rename(&tmp, &dest)?;
            Ok(InstallRecord {
                product_id: req.product.id.clone(),
                version: Some(req.version.to_string()),
                path: dest,
                source: InstallSource::Managed,
                installed_at: req.existing.as_ref().map(|r| r.installed_at).unwrap_or(now),
                updated_at: now,
            })
        }
    }
}

pub fn uninstall(product: &Product, rec: &InstallRecord) -> AppResult<()> {
    match &product.install {
        InstallSpec::Nsis { registry_name, main_binary, .. } => {
            if platform::exe_in_use(&rec.path.join(main_binary)) {
                return Err(AppError::Running);
            }
            let entry = platform::uninstall_entry(registry_name);
            let uninstaller = entry
                .and_then(|e| e.uninstaller)
                .or_else(|| Some(rec.path.join("uninstall.exe")).filter(|p| p.is_file()))
                .ok_or_else(|| AppError::InstallFailed("no uninstaller found for this installation".into()))?;
            let dir = uninstaller.parent().map(Path::to_path_buf).unwrap_or_else(|| rec.path.clone());
            let mut cmd = platform::command(&uninstaller);
            cmd.arg("/S");
            // `_?=` makes the NSIS uninstaller run in place and wait until it is done.
            platform::raw_arg(&mut cmd, &format!("_?={}", dir.display()));
            crate::logger::info(&format!("running uninstaller {}", uninstaller.display()));
            run_and_wait(cmd, "uninstaller")?;
            // With `_?=` the uninstaller cannot delete itself; clean up what is left.
            let _ = fs::remove_file(&uninstaller);
            let _ = fs::remove_dir(&dir);
            if dir.join(main_binary).exists() {
                return Err(AppError::InstallFailed("the uninstaller finished but files are still there".into()));
            }
            Ok(())
        }
        InstallSpec::Zip { main_binary, .. } => {
            if platform::exe_in_use(&rec.path.join(main_binary)) {
                return Err(AppError::Running);
            }
            if !rec.path.join(MARKER).is_file() {
                return Err(AppError::Invalid("this folder was not created by the Mooncrest Launcher, remove it yourself".into()));
            }
            fs::remove_dir_all(&rec.path)?;
            Ok(())
        }
        InstallSpec::File { .. } => {
            fs::remove_file(&rec.path)?;
            Ok(())
        }
    }
}

fn run_and_wait(mut cmd: std::process::Command, what: &str) -> AppResult<()> {
    let mut child = cmd.spawn().map_err(|e| AppError::InstallFailed(format!("could not start the {what}: {e}")))?;
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "?".into());
                return Err(AppError::InstallFailed(format!("the {what} ended with code {code}")));
            }
            None if started.elapsed() > INSTALLER_TIMEOUT => {
                let _ = child.kill();
                return Err(AppError::InstallFailed(format!("the {what} did not finish within 20 minutes")));
            }
            None => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

/// Extracts next to the target and swaps folders at the end, so a failed
/// extraction never leaves a half-updated product behind.
pub fn extract_zip(archive: &Path, target: &Path, strip_root: bool, product_id: &str, version: &str) -> AppResult<()> {
    let parent = target.parent().ok_or_else(|| AppError::Invalid("bad target".into()))?;
    fs::create_dir_all(parent)?;
    let name = target.file_name().ok_or_else(|| AppError::Invalid("bad target".into()))?.to_string_lossy().to_string();
    let staging = parent.join(format!("{name}.mooncrest-new"));
    let backup = parent.join(format!("{name}.mooncrest-old"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let result = (|| -> AppResult<()> {
        let f = fs::File::open(archive)?;
        let mut zip = zip::ZipArchive::new(f).map_err(|e| AppError::InstallFailed(format!("archive is damaged: {e}")))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| AppError::InstallFailed(e.to_string()))?;
            // enclosed_name() rejects absolute paths and `..` (zip-slip).
            let Some(rel) = entry.enclosed_name() else {
                return Err(AppError::InstallFailed(format!("unsafe path in archive: {}", entry.name())));
            };
            let rel: PathBuf = if strip_root { rel.components().skip(1).collect() } else { rel };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let out = staging.join(&rel);
            if entry.is_dir() {
                fs::create_dir_all(&out)?;
            } else {
                if let Some(p) = out.parent() {
                    fs::create_dir_all(p)?;
                }
                let mut w = fs::File::create(&out)?;
                std::io::copy(&mut entry, &mut w)?;
                #[cfg(unix)]
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o755));
                }
            }
        }
        store::write_json(&staging.join(MARKER), &Marker { product_id: product_id.into(), version: version.into() })?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let _ = fs::remove_dir_all(&backup);
    if target.exists() {
        fs::rename(target, &backup)?;
    }
    if let Err(e) = fs::rename(&staging, target) {
        let _ = fs::rename(&backup, target);
        return Err(e.into());
    }
    let _ = fs::remove_dir_all(&backup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let f = fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn extracts_with_strip_root_and_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.zip");
        make_zip(&a, &[("Game/game.exe", b"exe"), ("Game/data/x.bin", b"123")]);
        let target = tmp.path().join("lib").join("Game");
        extract_zip(&a, &target, true, "game", "1.0.0").unwrap();
        assert_eq!(fs::read(target.join("game.exe")).unwrap(), b"exe");
        assert_eq!(fs::read(target.join("data/x.bin")).unwrap(), b"123");
        assert_eq!(crate::library::read_marker(&target).as_deref(), Some("1.0.0"));

        // Update replaces the folder content completely.
        let b = tmp.path().join("b.zip");
        make_zip(&b, &[("Game/game.exe", b"exe2")]);
        extract_zip(&b, &target, true, "game", "1.1.0").unwrap();
        assert_eq!(fs::read(target.join("game.exe")).unwrap(), b"exe2");
        assert!(!target.join("data/x.bin").exists());
        assert!(!tmp.path().join("lib/Game.mooncrest-old").exists());
        assert!(!tmp.path().join("lib/Game.mooncrest-new").exists());
    }

    #[test]
    fn rejects_zip_slip_and_keeps_old_version() {
        let tmp = tempfile::tempdir().unwrap();
        let good = tmp.path().join("good.zip");
        make_zip(&good, &[("game.exe", b"v1")]);
        let target = tmp.path().join("lib").join("Game");
        extract_zip(&good, &target, false, "game", "1.0.0").unwrap();

        let evil = tmp.path().join("evil.zip");
        make_zip(&evil, &[("game.exe", b"v2"), ("../../escaped.txt", b"x")]);
        assert!(extract_zip(&evil, &target, false, "game", "2.0.0").is_err());
        assert!(!tmp.path().join("escaped.txt").exists());
        assert_eq!(fs::read(target.join("game.exe")).unwrap(), b"v1", "failed update must not touch the installed version");
    }

    #[test]
    fn rejects_damaged_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let bad = tmp.path().join("bad.zip");
        fs::write(&bad, b"not a zip").unwrap();
        assert!(matches!(extract_zip(&bad, &tmp.path().join("G"), false, "g", "1"), Err(AppError::InstallFailed(_))));
    }
}
