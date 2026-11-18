//! Download queue with resumable, verified downloads.
//!
//! * One download runs at a time, the rest wait in order.
//! * Data goes to `<file>.part`. After an interruption (pause, network drop,
//!   launcher closed) the download continues with an HTTP Range request.
//!   Servers that ignore Range simply start over.
//! * Size is checked against the catalog before and after; the finished file
//!   must match its SHA-256 or it is deleted and never installed.
//! * Network errors and 5xx answers are retried with back-off.
//! * The queue is saved to disk and restored on the next start.
//!
//! The engine knows nothing about Tauri; `Hooks` connects it to the app, which
//! keeps it testable against a local test server.

use crate::error::{AppError, AppResult, ErrorInfo};
use crate::store;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Install,
    Update,
    Launcher,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Downloading,
    Paused,
    Verifying,
    /// Downloaded and verified, waiting for the install step.
    Ready,
    /// Verified update waiting for the product to be closed.
    WaitingForExit,
    Installing,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(self, JobState::Completed | JobState::Failed | JobState::Cancelled)
    }
    pub fn is_active(self) -> bool {
        !self.is_terminal()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub product_id: String,
    pub product_name: String,
    pub version: String,
    pub kind: JobKind,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// Final path of the verified file.
    pub dest: PathBuf,
    /// Install from a file the user picked instead of downloading.
    #[serde(default)]
    pub local_source: Option<PathBuf>,
    #[serde(default)]
    pub install_dir: Option<PathBuf>,
    pub state: JobState,
    pub downloaded: u64,
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub eta_secs: Option<u64>,
    #[serde(default)]
    pub error: Option<ErrorInfo>,
    /// Short status detail (e.g. "retrying in 4 s").
    #[serde(default)]
    pub note: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub finished_at: Option<u64>,
    #[serde(default)]
    pub resumed: bool,
}

impl Job {
    pub fn part_path(&self) -> PathBuf {
        let mut s = self.dest.as_os_str().to_owned();
        s.push(".part");
        PathBuf::from(s)
    }
    /// The file the install step should use.
    pub fn payload(&self) -> &Path {
        self.local_source.as_deref().unwrap_or(&self.dest)
    }
}

pub struct NewJob {
    pub product_id: String,
    pub product_name: String,
    pub version: String,
    pub kind: JobKind,
    pub file_name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub dest_dir: PathBuf,
    pub local_source: Option<PathBuf>,
    pub install_dir: Option<PathBuf>,
}

pub trait Hooks: Send + Sync + 'static {
    /// The job list changed (throttled while bytes flow).
    fn changed(&self, jobs: &[Job]);
    /// A job reached `Ready`: the file is verified and can be installed.
    fn ready(&self, job: Job);
}

const CTRL_RUN: u8 = 0;
const CTRL_PAUSE: u8 = 1;
const CTRL_CANCEL: u8 = 2;

struct Control {
    flag: AtomicU8,
    wake: Notify,
}

struct Inner {
    jobs: Vec<Job>,
    controls: HashMap<String, Arc<Control>>,
    counter: u64,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Mutex<Inner>>,
    wake: Arc<Notify>,
    client: reqwest::Client,
    hooks: Arc<dyn Hooks>,
    persist: Option<PathBuf>,
    /// Serializes queue file writes so an older snapshot never overwrites a newer one.
    persist_lock: Arc<Mutex<()>>,
    retry_base: Duration,
}

const MAX_ATTEMPTS: u32 = 6;
const HISTORY_KEEP: usize = 40;

impl Engine {
    pub fn new(client: reqwest::Client, hooks: Arc<dyn Hooks>, persist: Option<PathBuf>) -> Engine {
        let mut jobs: Vec<Job> = persist.as_deref().and_then(|p| store::read_json(p).ok().flatten()).unwrap_or_default();
        // Restore after a restart: interrupted transfers continue, an interrupted
        // install is marked failed (the verified file is still there for "retry").
        for j in &mut jobs {
            j.speed = 0.0;
            j.eta_secs = None;
            match j.state {
                JobState::Downloading | JobState::Verifying => {
                    j.state = JobState::Queued;
                    j.resumed = true;
                }
                JobState::Installing => {
                    j.state = JobState::Failed;
                    j.error = Some(ErrorInfo { code: "interrupted".into(), message: "the launcher was closed during installation".into() });
                }
                _ => {}
            }
        }
        let engine = Engine {
            inner: Arc::new(Mutex::new(Inner { jobs, controls: HashMap::new(), counter: store::now_ms() })),
            wake: Arc::new(Notify::new()),
            client,
            hooks,
            persist,
            persist_lock: Arc::new(Mutex::new(())),
            retry_base: Duration::from_secs(1),
        };
        engine
    }

    /// Shorter back-off for tests.
    pub fn with_retry_base(mut self, d: Duration) -> Self {
        self.retry_base = d;
        self
    }

    /// Starts the worker. Jobs left in `Ready`/`WaitingForExit` are handed to the hooks again.
    pub fn start(&self) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move { me.worker().await });
        let pending: Vec<Job> = self.snapshot().into_iter().filter(|j| j.state == JobState::Ready).collect();
        for j in pending {
            self.hooks.ready(j);
        }
    }

    pub fn snapshot(&self) -> Vec<Job> {
        self.inner.lock().unwrap().jobs.clone()
    }

    pub fn get(&self, id: &str) -> Option<Job> {
        self.inner.lock().unwrap().jobs.iter().find(|j| j.id == id).cloned()
    }

    /// The active (non-finished) job for a product, if any.
    pub fn active_for(&self, product_id: &str) -> Option<Job> {
        self.inner.lock().unwrap().jobs.iter().find(|j| j.product_id == product_id && j.state.is_active()).cloned()
    }

    pub fn enqueue(&self, n: NewJob) -> AppResult<Job> {
        let file_name = store::safe_file_name(&n.file_name)?;
        let job = {
            let mut g = self.inner.lock().unwrap();
            if g.jobs.iter().any(|j| j.product_id == n.product_id && j.state.is_active()) {
                return Err(AppError::Busy);
            }
            g.counter += 1;
            let now = store::now_ms();
            let job = Job {
                id: format!("j{}", g.counter),
                product_id: n.product_id,
                product_name: n.product_name,
                version: n.version,
                kind: n.kind,
                dest: n.dest_dir.join(&file_name),
                file_name,
                url: n.url,
                sha256: n.sha256.to_ascii_lowercase(),
                size: n.size,
                local_source: n.local_source,
                install_dir: n.install_dir,
                state: JobState::Queued,
                downloaded: 0,
                speed: 0.0,
                eta_secs: None,
                error: None,
                note: None,
                created_at: now,
                updated_at: now,
                finished_at: None,
                resumed: false,
            };
            g.jobs.push(job.clone());
            job
        };
        self.commit();
        self.wake.notify_one();
        Ok(job)
    }

    pub fn pause(&self, id: &str) -> AppResult<()> {
        let mut g = self.inner.lock().unwrap();
        let job = g.jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| AppError::Invalid("unknown job".into()))?;
        match job.state {
            JobState::Queued => {
                job.state = JobState::Paused;
                job.speed = 0.0;
            }
            JobState::Downloading | JobState::Verifying => {
                if let Some(c) = g.controls.get(id) {
                    c.flag.store(CTRL_PAUSE, Ordering::SeqCst);
                    c.wake.notify_waiters();
                }
            }
            _ => return Err(AppError::Invalid("this job cannot be paused now".into())),
        }
        drop(g);
        self.commit();
        Ok(())
    }

    pub fn resume(&self, id: &str) -> AppResult<()> {
        self.update(id, |j| {
            if matches!(j.state, JobState::Paused | JobState::Failed) {
                j.state = JobState::Queued;
                j.error = None;
                j.note = None;
                j.finished_at = None;
                Ok(())
            } else {
                Err(AppError::Invalid("this job cannot be resumed".into()))
            }
        })?;
        self.wake.notify_one();
        Ok(())
    }

    pub fn cancel(&self, id: &str) -> AppResult<()> {
        let running = {
            let mut g = self.inner.lock().unwrap();
            let ctrl = g.controls.get(id).cloned();
            let job = g.jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| AppError::Invalid("unknown job".into()))?;
            if matches!(job.state, JobState::Installing) || job.state.is_terminal() {
                return Err(AppError::Invalid("this job cannot be cancelled now".into()));
            }
            if let Some(c) = ctrl {
                c.flag.store(CTRL_CANCEL, Ordering::SeqCst);
                c.wake.notify_waiters();
                true
            } else {
                job.state = JobState::Cancelled;
                job.finished_at = Some(store::now_ms());
                job.speed = 0.0;
                remove_files(job);
                false
            }
        };
        if !running {
            self.commit();
        }
        Ok(())
    }

    /// Removes a finished job from the history.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        {
            let mut g = self.inner.lock().unwrap();
            let before = g.jobs.len();
            g.jobs.retain(|j| !(j.id == id && j.state.is_terminal()));
            if g.jobs.len() == before {
                return Err(AppError::Invalid("only finished jobs can be removed".into()));
            }
        }
        self.commit();
        Ok(())
    }

    pub fn clear_history(&self) {
        self.inner.lock().unwrap().jobs.retain(|j| !j.state.is_terminal());
        self.commit();
    }

    /// Updates a job and notifies listeners.
    pub fn update<R>(&self, id: &str, f: impl FnOnce(&mut Job) -> AppResult<R>) -> AppResult<R> {
        let r = {
            let mut g = self.inner.lock().unwrap();
            let job = g.jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| AppError::Invalid("unknown job".into()))?;
            let r = f(job)?;
            job.updated_at = store::now_ms();
            if job.state.is_terminal() && job.finished_at.is_none() {
                job.finished_at = Some(job.updated_at);
            }
            r
        };
        self.commit();
        Ok(r)
    }

    fn set_state(&self, id: &str, state: JobState) {
        let _ = self.update(id, |j| {
            j.state = state;
            if state != JobState::Downloading {
                j.speed = 0.0;
                j.eta_secs = None;
            }
            Ok(())
        });
    }

    fn fail(&self, id: &str, e: &AppError) {
        crate::logger::warn(&format!("job {id} failed: {e}"));
        let _ = self.update(id, |j| {
            j.state = JobState::Failed;
            j.error = Some(ErrorInfo::from(e));
            j.note = None;
            j.speed = 0.0;
            j.eta_secs = None;
            Ok(())
        });
    }

    /// Persists (keeping a bounded history) and notifies listeners.
    fn commit(&self) {
        let _write = self.persist_lock.lock().unwrap();
        let jobs = {
            let mut g = self.inner.lock().unwrap();
            let finished = g.jobs.iter().filter(|j| j.state.is_terminal()).count();
            if finished > HISTORY_KEEP {
                let mut drop_n = finished - HISTORY_KEEP;
                g.jobs.retain(|j| {
                    if drop_n > 0 && j.state.is_terminal() {
                        drop_n -= 1;
                        false
                    } else {
                        true
                    }
                });
            }
            g.jobs.clone()
        };
        if let Some(p) = &self.persist {
            if let Err(e) = store::write_json(p, &jobs) {
                crate::logger::error(&format!("saving download queue failed: {e}"));
            }
        }
        self.hooks.changed(&jobs);
    }

    fn emit_progress(&self) {
        let jobs = self.snapshot();
        self.hooks.changed(&jobs);
    }

    // ------------------------------------------------------------ worker

    async fn worker(&self) {
        loop {
            let next = {
                let g = self.inner.lock().unwrap();
                g.jobs.iter().find(|j| j.state == JobState::Queued).cloned()
            };
            let Some(job) = next else {
                self.wake.notified().await;
                continue;
            };
            let ctrl = Arc::new(Control { flag: AtomicU8::new(CTRL_RUN), wake: Notify::new() });
            self.inner.lock().unwrap().controls.insert(job.id.clone(), ctrl.clone());
            let result = self.run(&job, &ctrl).await;
            self.inner.lock().unwrap().controls.remove(&job.id);
            match result {
                Ok(()) => {
                    self.set_state(&job.id, JobState::Ready);
                    if let Some(j) = self.get(&job.id) {
                        self.hooks.ready(j);
                    }
                }
                Err(AppError::Cancelled) => match ctrl.flag.load(Ordering::SeqCst) {
                    CTRL_CANCEL => {
                        let _ = self.update(&job.id, |j| {
                            j.state = JobState::Cancelled;
                            j.speed = 0.0;
                            j.eta_secs = None;
                            remove_files(j);
                            Ok(())
                        });
                    }
                    _ => self.set_state(&job.id, JobState::Paused),
                },
                Err(e) => self.fail(&job.id, &e),
            }
        }
    }

    async fn run(&self, job: &Job, ctrl: &Arc<Control>) -> AppResult<()> {
        if let Some(src) = &job.local_source {
            // Picked by the user: only verify, never copy or trust blindly.
            return self.verify(job, src, ctrl).await;
        }
        if let Some(dir) = job.dest.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        // Already downloaded and verified earlier (e.g. retry after a failed install).
        if tokio::fs::metadata(&job.dest).await.map(|m| m.len() == job.size).unwrap_or(false) {
            match self.verify(job, &job.dest, ctrl).await {
                Ok(()) => return Ok(()),
                Err(AppError::Cancelled) => return Err(AppError::Cancelled),
                Err(_) => {
                    let _ = tokio::fs::remove_file(&job.dest).await;
                }
            }
        }
        let have = tokio::fs::metadata(job.part_path()).await.map(|m| m.len()).unwrap_or(0);
        let needed = job.size.saturating_sub(have);
        if let Some(free) = job.dest.parent().and_then(store::free_space) {
            if free < needed + 16 * 1024 * 1024 {
                return Err(AppError::DiskFull { needed, free });
            }
        }

        let mut redownloads = 0;
        loop {
            self.transfer_with_retries(job, ctrl).await?;
            let part = job.part_path();
            match self.verify(job, &part, ctrl).await {
                Ok(()) => {
                    tokio::fs::rename(&part, &job.dest).await?;
                    return Ok(());
                }
                Err(e @ AppError::VerifyFailed { .. }) => {
                    let _ = tokio::fs::remove_file(&part).await;
                    if redownloads >= 1 {
                        return Err(e);
                    }
                    redownloads += 1;
                    crate::logger::warn(&format!("{}: checksum mismatch, downloading once more from scratch", job.file_name));
                    let _ = self.update(&job.id, |j| {
                        j.downloaded = 0;
                        Ok(())
                    });
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn transfer_with_retries(&self, job: &Job, ctrl: &Arc<Control>) -> AppResult<()> {
        let mut attempt = 0u32;
        loop {
            self.set_state(&job.id, JobState::Downloading);
            match self.transfer(job, ctrl).await {
                Ok(()) => return Ok(()),
                Err(e) if e.is_transient() && attempt + 1 < MAX_ATTEMPTS => {
                    attempt += 1;
                    let wait = self.retry_base * 2u32.pow(attempt - 1);
                    crate::logger::warn(&format!("{}: {e} – retry {attempt} in {wait:?}", job.file_name));
                    let note = format!("retry {}/{} in {} s", attempt, MAX_ATTEMPTS - 1, wait.as_secs().max(1));
                    let _ = self.update(&job.id, |j| {
                        j.note = Some(note);
                        j.speed = 0.0;
                        Ok(())
                    });
                    tokio::select! {
                        _ = tokio::time::sleep(wait) => {}
                        _ = ctrl.wake.notified() => {}
                    }
                    if ctrl.flag.load(Ordering::SeqCst) != CTRL_RUN {
                        return Err(AppError::Cancelled);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// One HTTP attempt. Appends to `.part`, resuming where it left off.
    async fn transfer(&self, job: &Job, ctrl: &Arc<Control>) -> AppResult<()> {
        let part = job.part_path();
        let mut offset = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
        if offset > job.size {
            tokio::fs::remove_file(&part).await?;
            offset = 0;
        }
        if offset == job.size {
            return Ok(());
        }

        let mut req = self.client.get(&job.url);
        if offset > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let send = req.send();
        let resp = tokio::select! {
            r = send => r?,
            _ = ctrl.wake.notified() => return Err(AppError::Cancelled),
        };
        let status = resp.status().as_u16();
        match status {
            404 | 410 => return Err(AppError::NotFound),
            416 => {
                // Our partial file does not fit the server's file: start over.
                tokio::fs::remove_file(&part).await.ok();
                return Err(AppError::Network("range not satisfiable, restarting".into()));
            }
            200 | 206 => {}
            s => return Err(AppError::HttpStatus(s)),
        }
        let ctype = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
        if ctype.starts_with("text/html") {
            return Err(AppError::NotAvailable("the server returned a web page instead of the file".into()));
        }

        let append = status == 206 && offset > 0 && content_range_start(&resp) == Some(offset);
        if !append {
            offset = 0;
        }
        if let Some(len) = resp.content_length() {
            let total = offset + len;
            if total != job.size {
                return Err(AppError::SizeMismatch { expected: job.size, actual: total });
            }
        }

        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&part)
            .await?;
        let mut out = tokio::io::BufWriter::with_capacity(256 * 1024, file);
        let _ = self.update(&job.id, |j| {
            j.downloaded = offset;
            j.note = None;
            if append {
                j.resumed = true;
            }
            Ok(())
        });

        let mut done = offset;
        let mut stream = resp.bytes_stream();
        let mut samples: VecDeque<(Instant, u64)> = VecDeque::new();
        samples.push_back((Instant::now(), done));
        let mut last_emit = Instant::now();
        loop {
            let next = tokio::select! {
                n = tokio::time::timeout(Duration::from_secs(30), stream.next()) => n,
                _ = ctrl.wake.notified() => {
                    out.flush().await?;
                    return Err(AppError::Cancelled);
                }
            };
            let chunk = match next {
                Err(_) => {
                    out.flush().await?;
                    return Err(AppError::Network("no data for 30 seconds".into()));
                }
                Ok(None) => break,
                Ok(Some(Err(e))) => {
                    out.flush().await?;
                    return Err(AppError::Network(e.to_string()));
                }
                Ok(Some(Ok(c))) => c,
            };
            if done + chunk.len() as u64 > job.size {
                out.flush().await?;
                drop(out);
                tokio::fs::remove_file(&part).await.ok();
                return Err(AppError::SizeMismatch { expected: job.size, actual: done + chunk.len() as u64 });
            }
            if let Err(e) = out.write_all(&chunk).await {
                return Err(if is_disk_full(&e) { AppError::DiskFull { needed: job.size - done, free: 0 } } else { e.into() });
            }
            done += chunk.len() as u64;
            if ctrl.flag.load(Ordering::SeqCst) != CTRL_RUN {
                out.flush().await?;
                return Err(AppError::Cancelled);
            }
            if last_emit.elapsed() >= Duration::from_millis(200) {
                let now = Instant::now();
                samples.push_back((now, done));
                while samples.len() > 2 && now.duration_since(samples[0].0) > Duration::from_secs(3) {
                    samples.pop_front();
                }
                let (t0, b0) = samples[0];
                let secs = now.duration_since(t0).as_secs_f64();
                let speed = if secs > 0.0 { (done - b0) as f64 / secs } else { 0.0 };
                let eta = if speed > 1.0 { Some(((job.size - done) as f64 / speed).ceil() as u64) } else { None };
                {
                    let mut g = self.inner.lock().unwrap();
                    if let Some(j) = g.jobs.iter_mut().find(|j| j.id == job.id) {
                        j.downloaded = done;
                        j.speed = speed;
                        j.eta_secs = eta;
                    }
                }
                self.emit_progress();
                last_emit = now;
            }
        }
        out.flush().await?;
        out.get_ref().sync_all().await.ok();
        let _ = self.update(&job.id, |j| {
            j.downloaded = done;
            Ok(())
        });
        if done != job.size {
            return Err(AppError::Network(format!("connection closed early ({done} of {} bytes)", job.size)));
        }
        Ok(())
    }

    /// SHA-256 + size check, with progress, cancellable.
    async fn verify(&self, job: &Job, path: &Path, ctrl: &Arc<Control>) -> AppResult<()> {
        self.set_state(&job.id, JobState::Verifying);
        let meta = tokio::fs::metadata(path).await.map_err(|e| AppError::Io(format!("{}: {e}", path.display())))?;
        if meta.len() != job.size {
            return Err(AppError::SizeMismatch { expected: job.size, actual: meta.len() });
        }
        let path = path.to_path_buf();
        let me = self.clone();
        let id = job.id.clone();
        let c = ctrl.clone();
        let digest = tokio::task::spawn_blocking(move || -> AppResult<String> {
            use std::io::Read;
            let mut f = std::fs::File::open(&path)?;
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 1024 * 1024];
            let mut read = 0u64;
            let mut last = Instant::now();
            loop {
                if c.flag.load(Ordering::SeqCst) != CTRL_RUN {
                    return Err(AppError::Cancelled);
                }
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                read += n as u64;
                if last.elapsed() >= Duration::from_millis(200) {
                    {
                        let mut g = me.inner.lock().unwrap();
                        if let Some(j) = g.jobs.iter_mut().find(|j| j.id == id) {
                            j.downloaded = read;
                        }
                    }
                    me.emit_progress();
                    last = Instant::now();
                }
            }
            Ok(hex::encode(h.finalize()))
        })
        .await
        .map_err(|e| AppError::Io(e.to_string()))??;
        if digest != job.sha256 {
            return Err(AppError::VerifyFailed { expected: job.sha256.clone(), actual: digest });
        }
        let _ = self.update(&job.id, |j| {
            j.downloaded = j.size;
            Ok(())
        });
        Ok(())
    }
}

fn content_range_start(resp: &reqwest::Response) -> Option<u64> {
    // "bytes 1000-1999/2000"
    let v = resp.headers().get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let rest = v.strip_prefix("bytes ")?;
    rest.split('-').next()?.trim().parse().ok()
}

fn is_disk_full(e: &std::io::Error) -> bool {
    // ERROR_DISK_FULL (112) on Windows, ENOSPC (28) on Unix.
    matches!(e.raw_os_error(), Some(112) | Some(28))
}

fn remove_files(job: &Job) {
    let _ = std::fs::remove_file(job.part_path());
    if job.local_source.is_none() && job.kind != JobKind::Launcher {
        let _ = std::fs::remove_file(&job.dest);
    }
}

pub fn sha256_file(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}
