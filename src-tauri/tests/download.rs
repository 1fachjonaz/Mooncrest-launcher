//! Download engine against a local HTTP server that can misbehave on purpose.

use mooncrest_launcher_lib::{Engine, Hooks, Job, JobKind, JobState, NewJob};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    /// First request is cut after this many bytes, later ones work (with Range).
    DropOnce(usize),
    /// Ignores Range headers, always sends the whole file with 200.
    NoRange(usize),
    Html,
    NotFound,
    /// Serves a file with one flipped byte.
    Corrupt,
    /// Sends data slowly so the test can pause/cancel mid-way.
    Slow,
    /// Answers 503 twice, then works.
    Flaky,
}

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
}

async fn serve(data: Arc<Vec<u8>>, mode: Mode) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let ranges = Arc::new(Mutex::new(vec![]));
    let (rq, rg) = (requests.clone(), ranges.clone());
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = listener.accept().await.unwrap();
            let n = rq.fetch_add(1, Ordering::SeqCst);
            let data = data.clone();
            let rg = rg.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut req = Vec::new();
                loop {
                    let k = sock.read(&mut buf).await.unwrap_or(0);
                    if k == 0 { return; }
                    req.extend_from_slice(&buf[..k]);
                    if req.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                }
                let text = String::from_utf8_lossy(&req).to_string();
                let range = text.lines().find_map(|l| {
                    let l = l.to_ascii_lowercase();
                    l.strip_prefix("range: bytes=").map(|r| r.trim_end_matches('-').trim().to_string())
                });
                rg.lock().unwrap().push(range.clone());
                let start: usize = range.as_deref().and_then(|r| r.parse().ok()).unwrap_or(0);
                let send = |status: &str, ctype: &str, body: &[u8], extra: String| {
                    let mut h = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", body.len()).into_bytes();
                    h.extend_from_slice(body);
                    h
                };
                match mode {
                    Mode::NotFound => { let _ = sock.write_all(&send("404 Not Found", "text/plain", b"nope", String::new())).await; }
                    Mode::Html => { let _ = sock.write_all(&send("200 OK", "text/html; charset=utf-8", b"<html>soon</html>", String::new())).await; }
                    Mode::Flaky if n < 2 => { let _ = sock.write_all(&send("503 Service Unavailable", "text/plain", b"busy", String::new())).await; }
                    _ => {
                        let mut body = data.to_vec();
                        if mode == Mode::Corrupt { let mid = body.len() / 2; body[mid] ^= 0xff; }
                        let ignore_range = matches!(mode, Mode::NoRange(_));
                        let (status, slice, extra) = if start > 0 && !ignore_range {
                            ("206 Partial Content", body[start..].to_vec(), format!("Content-Range: bytes {}-{}/{}\r\n", start, body.len() - 1, body.len()))
                        } else {
                            ("200 OK", body.clone(), "Accept-Ranges: bytes\r\n".to_string())
                        };
                        let head = format!("HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", slice.len());
                        if sock.write_all(head.as_bytes()).await.is_err() { return; }
                        let cut = match mode { Mode::DropOnce(c) | Mode::NoRange(c) if n == 0 => Some(c), _ => None };
                        let mut sent = 0;
                        for chunk in slice.chunks(16 * 1024) {
                            if let Some(c) = cut { if sent + chunk.len() > c { let _ = sock.write_all(&chunk[..c - sent]).await; let _ = sock.shutdown().await; return; } }
                            if sock.write_all(chunk).await.is_err() { return; }
                            sent += chunk.len();
                            if mode == Mode::Slow { tokio::time::sleep(Duration::from_millis(20)).await; }
                        }
                    }
                }
                let _ = sock.shutdown().await;
            });
        }
    });
    Server { url: format!("http://{addr}/file.bin"), requests, ranges }
}

struct TestHooks {
    ready: Mutex<Vec<Job>>,
}
impl Hooks for TestHooks {
    fn changed(&self, _jobs: &[Job]) {}
    fn ready(&self, job: Job) { self.ready.lock().unwrap().push(job); }
}

fn payload(len: usize) -> (Arc<Vec<u8>>, String) {
    let data: Vec<u8> = (0..len).map(|i| ((i * 31 + i / 7) % 251) as u8).collect();
    let hash = hex::encode(Sha256::digest(&data));
    (Arc::new(data), hash)
}

fn engine(dir: &std::path::Path) -> (Engine, Arc<TestHooks>) {
    let hooks = Arc::new(TestHooks { ready: Mutex::new(vec![]) });
    let client = reqwest::Client::builder().build().unwrap();
    let e = Engine::new(client, hooks.clone(), Some(dir.join("jobs.json"))).with_retry_base(Duration::from_millis(50));
    e.start();
    (e, hooks)
}

fn job(url: &str, hash: &str, size: u64, dir: &std::path::Path) -> NewJob {
    NewJob {
        product_id: "test".into(), product_name: "Test".into(), version: "1.0.0".into(), kind: JobKind::Install,
        file_name: "file.bin".into(), url: url.into(), sha256: hash.into(), size, dest_dir: dir.join("dl"),
        local_source: None, install_dir: None,
    }
}

async fn wait_for(e: &Engine, id: &str, pred: impl Fn(&Job) -> bool) -> Job {
    for _ in 0..600 {
        if let Some(j) = e.get(id) { if pred(&j) { return j; } }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timeout, job = {:?}", e.get(id));
}

fn done(j: &Job) -> bool { matches!(j.state, JobState::Ready | JobState::Failed | JobState::Cancelled) }

#[tokio::test]
async fn downloads_and_verifies() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(700_000);
    let srv = serve(data.clone(), Mode::Normal).await;
    let (e, hooks) = engine(tmp.path());
    let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Ready, "{:?}", j.error);
    assert_eq!(std::fs::read(&j.dest).unwrap(), *data);
    assert!(!j.part_path().exists());
    assert_eq!(hooks.ready.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn resumes_after_connection_drop() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(900_000);
    let srv = serve(data.clone(), Mode::DropOnce(300_000)).await;
    let (e, _) = engine(tmp.path());
    let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Ready, "{:?}", j.error);
    assert_eq!(std::fs::read(&j.dest).unwrap(), *data);
    let ranges = srv.ranges.lock().unwrap().clone();
    assert_eq!(ranges[0], None);
    assert_eq!(ranges[1].as_deref(), Some("300000"), "second request must resume with Range: {ranges:?}");
    assert!(j.resumed);
}

#[tokio::test]
async fn restarts_when_server_ignores_range() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(500_000);
    let srv = serve(data.clone(), Mode::NoRange(200_000)).await;
    let (e, _) = engine(tmp.path());
    let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Ready, "{:?}", j.error);
    assert_eq!(std::fs::read(&j.dest).unwrap(), *data);
}

#[tokio::test]
async fn rejects_corrupt_file() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(300_000);
    let srv = serve(data.clone(), Mode::Corrupt).await;
    let (e, hooks) = engine(tmp.path());
    let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Failed);
    assert_eq!(j.error.as_ref().unwrap().code, "verify_failed");
    assert!(!j.dest.exists() && !j.part_path().exists(), "bad files must be deleted");
    assert_eq!(srv.requests.load(Ordering::SeqCst), 2, "one clean re-download before giving up");
    assert!(hooks.ready.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rejects_wrong_size_html_and_404() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(100_000);
    let (e, _) = engine(tmp.path());

    let srv = serve(data.clone(), Mode::Normal).await;
    let j = e.enqueue(job(&srv.url, &hash, 99_999, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.error.unwrap().code, "size_mismatch");

    let srv = serve(data.clone(), Mode::Html).await;
    let j = e.enqueue(job(&srv.url, &hash, 100_000, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.error.unwrap().code, "not_available");

    let srv = serve(data.clone(), Mode::NotFound).await;
    let j = e.enqueue(job(&srv.url, &hash, 100_000, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.error.unwrap().code, "not_found");
}

#[tokio::test]
async fn retries_server_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(120_000);
    let srv = serve(data.clone(), Mode::Flaky).await;
    let (e, _) = engine(tmp.path());
    let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Ready, "{:?}", j.error);
    assert_eq!(srv.requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn pause_resume_and_cancel() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(2_000_000);
    let srv = serve(data.clone(), Mode::Slow).await;
    let (e, _) = engine(tmp.path());
    let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    wait_for(&e, &j.id, |j| j.state == JobState::Downloading && j.downloaded > 200_000).await;
    e.pause(&j.id).unwrap();
    let p = wait_for(&e, &j.id, |j| j.state == JobState::Paused).await;
    let part_len = std::fs::metadata(p.part_path()).unwrap().len();
    assert!(part_len > 0 && part_len < data.len() as u64, "partial data kept on pause ({part_len})");
    e.resume(&j.id).unwrap();
    let r = wait_for(&e, &j.id, done).await;
    assert_eq!(r.state, JobState::Ready, "{:?}", r.error);
    assert_eq!(std::fs::read(&r.dest).unwrap(), *data);
    assert!(srv.ranges.lock().unwrap().iter().any(|r| r.is_some()), "resume used Range");

    // Cancel removes the partial file. Another product id avoids the "busy" guard.
    let mut nj = job(&srv.url, &hash, data.len() as u64, tmp.path());
    nj.product_id = "other".into();
    nj.file_name = "other.bin".into();
    let c = e.enqueue(nj).unwrap();
    wait_for(&e, &c.id, |j| j.state == JobState::Downloading && j.downloaded > 100_000).await;
    e.cancel(&c.id).unwrap();
    let c = wait_for(&e, &c.id, |j| j.state == JobState::Cancelled).await;
    assert!(!c.part_path().exists() && !c.dest.exists());
}

#[tokio::test]
async fn survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(1_500_000);
    let srv = serve(data.clone(), Mode::Slow).await;
    let id;
    {
        let (e, _) = engine(tmp.path());
        let j = e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
        id = j.id.clone();
        wait_for(&e, &id, |j| j.downloaded > 150_000).await;
        // Stop the transfer, then make the saved queue look like the app was
        // killed mid-download (state "downloading" with a .part file on disk).
        e.pause(&id).unwrap();
        wait_for(&e, &id, |j| j.state == JobState::Paused).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut saved: Vec<Job> = serde_json::from_slice(&std::fs::read(tmp.path().join("jobs.json")).unwrap()).unwrap();
        for s in &mut saved { s.state = JobState::Downloading; }
        std::fs::write(tmp.path().join("jobs.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
    }
    let (e2, _) = engine(tmp.path());
    let j = wait_for(&e2, &id, done).await;
    assert_eq!(j.state, JobState::Ready, "{:?}", j.error);
    assert_eq!(std::fs::read(&j.dest).unwrap(), *data);
    assert!(srv.ranges.lock().unwrap().iter().any(|r| r.is_some()), "continued with Range after restart");
}

#[tokio::test]
async fn local_file_is_verified_not_downloaded() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(250_000);
    let local = tmp.path().join("setup.exe");
    std::fs::write(&local, &*data).unwrap();
    let (e, _) = engine(tmp.path());
    let mut nj = job("https://unreachable.invalid/x", &hash, data.len() as u64, tmp.path());
    nj.local_source = Some(local.clone());
    let j = e.enqueue(nj).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Ready, "{:?}", j.error);
    assert_eq!(j.payload(), local.as_path());

    std::fs::write(&local, b"tampered").unwrap();
    let mut nj = job("https://unreachable.invalid/x", &hash, data.len() as u64, tmp.path());
    nj.product_id = "t2".into();
    nj.local_source = Some(local);
    let j = e.enqueue(nj).unwrap();
    let j = wait_for(&e, &j.id, done).await;
    assert_eq!(j.state, JobState::Failed);
}

#[tokio::test]
async fn one_active_job_per_product() {
    let tmp = tempfile::tempdir().unwrap();
    let (data, hash) = payload(1_000_000);
    let srv = serve(data.clone(), Mode::Slow).await;
    let (e, _) = engine(tmp.path());
    e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).unwrap();
    assert!(e.enqueue(job(&srv.url, &hash, data.len() as u64, tmp.path())).is_err());
}
