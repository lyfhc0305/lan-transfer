//! Sending and receiving. One connection carries one batch — any mix of files
//! and folders, or a text message — and the receiver confirms it once.
//!
//! Protocol (after the handshake in [`crate::wire`]):
//! 1. sender → `Offer` (the full list of entries, or the text)
//! 2. receiver → `Reply` (accepted or declined, after asking the user unless
//!    the sender is a trusted device)
//! 3. for every file, in list order: data frames, then its SHA-256
//! 4. receiver → `Receipt` once everything is saved
use crate::{
    model::*,
    wire::{self, fail, About, Error, Remote, Result, Secure},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// Plaintext bytes per data frame.
const CHUNK: usize = 64_000;
pub const MAX_ENTRIES: usize = 10_000;
const MAX_FILE: u64 = 100 * 1024 * 1024 * 1024;
pub const MAX_TEXT: usize = 256 * 1024;
/// Unread texts held at once: each one opens a dialog and brings the window
/// forward, so a peer must not be able to stack up arbitrarily many.
const MAX_PENDING_TEXTS: usize = 8;
const MAX_PATH_BYTES: usize = 1024;
const MAX_DEPTH: usize = 64;
const MAX_OFFER: usize = 16 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 8;
/// Concurrent connections accepted from a single address, so one computer
/// cannot hold every receiving slot (see the comment on [`MAX_CONNECTIONS`]).
const MAX_PER_IP: usize = 3;

/// One file or folder in a batch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// Relative path with "/" separators; the first component is the item
    /// the user picked.
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub dir: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Offer {
    Files { entries: Vec<Entry> },
    Text { text: String },
}

#[derive(Serialize, Deserialize)]
struct Reply {
    accept: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    ok: bool,
    #[serde(default)]
    reason: String,
}

/// A transfer that ended on purpose (declined, cancelled by the other side)
/// rather than failing.
#[derive(Debug)]
struct Ended(Stage, String);
impl std::fmt::Display for Ended {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}
impl std::error::Error for Ended {}

fn ended(stage: Stage, text: impl Into<String>) -> Error {
    Box::new(Ended(stage, text.into()))
}

/// A short Chinese explanation of an error for the interface.
pub fn describe(e: &(dyn std::error::Error + 'static)) -> String {
    if e.is::<Ended>() || e.is::<wire::Incompatible>() {
        return e.to_string();
    }
    if e.is::<snow::Error>() {
        return "加密校验失败：数据可能在传输中损坏，或对方的邻传版本不同".into();
    }
    if e.is::<serde_json::Error>() {
        return "收到无法识别的数据".into();
    }
    let Some(io) = e.downcast_ref::<io::Error>() else {
        return e.to_string();
    };
    if io.get_ref().is_some() {
        return io.to_string();
    }
    use io::ErrorKind::*;
    match io.kind() {
        ConnectionRefused => "对方没有响应：请确认对方已打开邻传".into(),
        TimedOut | WouldBlock => "连接超时，请检查两台电脑是否在同一网络".into(),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof => {
            "连接已断开：对方可能已取消，或网络中断".into()
        }
        HostUnreachable | NetworkUnreachable | AddrNotAvailable => {
            "无法连接到对方，请确认两台电脑在同一局域网".into()
        }
        NotFound => "找不到文件".into(),
        PermissionDenied => "没有访问权限".into(),
        StorageFull => "磁盘空间不足".into(),
        _ => io.to_string(),
    }
}

// ───────────────────────────── names and paths ─────────────────────────────

/// A single path component that is safe to create on macOS and Windows.
fn safe_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 180
        || name == "."
        || name == ".."
        || name.ends_with(['.', ' '])
        || name.chars().any(|c| {
            c.is_control()
                || "<>:\"/\\|?*".contains(c)
                // Bidirectional overrides can disguise a file's real extension.
                || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
    {
        return false;
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    !matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(1..=9).any(|i| base == format!("COM{i}") || base == format!("LPT{i}"))
}

/// System files that are not worth sending.
pub fn is_junk(name: &str) -> bool {
    matches!(
        name,
        ".DS_Store" | "Thumbs.db" | "desktop.ini" | ".localized" | "Icon\r"
    ) || name.starts_with("._")
}

/// "报告.pdf" → "报告 (2).pdf"; folders get the number at the end.
fn numbered(name: &str, n: usize, dir: bool) -> String {
    if !dir {
        if let Some(dot) = name.rfind('.').filter(|&i| i > 0) {
            return format!("{} ({n}){}", &name[..dot], &name[dot..]);
        }
    }
    format!("{name} ({n})")
}

fn native(relative: &str) -> PathBuf {
    relative.split('/').collect()
}

/// Check a received list of entries before anything touches the disk.
fn validate(entries: &[Entry]) -> Result<()> {
    if entries.is_empty() {
        return Err(fail("请求中没有文件"));
    }
    if entries.len() > MAX_ENTRIES {
        return Err(fail(format!("一次最多接收 {MAX_ENTRIES} 个文件和文件夹")));
    }
    let mut seen = HashSet::new();
    let mut files = HashSet::new();
    let mut total: u64 = 0;
    for e in entries {
        let parts: Vec<&str> = e.path.split('/').collect();
        if e.path.len() > MAX_PATH_BYTES
            || parts.len() > MAX_DEPTH
            || !parts.iter().all(|p| safe_name(p))
        {
            return Err(fail("请求中包含不支持的文件名"));
        }
        if e.dir && e.size != 0 {
            return Err(fail("收到异常的请求"));
        }
        if e.size > MAX_FILE {
            return Err(fail("单个文件不能超过 100 GB"));
        }
        // macOS and Windows file systems ignore case.
        let key = e.path.to_lowercase();
        if !seen.insert(key.clone()) {
            return Err(fail("请求中有重复的文件"));
        }
        if !e.dir {
            files.insert(key);
        }
        total = total
            .checked_add(e.size)
            .ok_or_else(|| fail("收到异常的请求"))?;
    }
    // A file cannot also be the folder of another entry.
    for e in entries {
        let key = e.path.to_lowercase();
        let mut end = 0;
        while let Some(i) = key[end..].find('/') {
            end += i;
            if files.contains(&key[..end]) {
                return Err(fail("收到异常的请求"));
            }
            end += 1;
        }
    }
    Ok(())
}

/// Top-level items of a batch with their total size and file count.
pub fn summarize(entries: &[Entry]) -> Vec<ItemSummary> {
    let mut items: Vec<ItemSummary> = vec![];
    let mut index: HashMap<&str, usize> = HashMap::new();
    for e in entries {
        let (top, nested) = match e.path.split_once('/') {
            Some((top, _)) => (top, true),
            None => (e.path.as_str(), false),
        };
        let i = *index.entry(top).or_insert_with(|| {
            items.push(ItemSummary {
                name: top.to_owned(),
                dir: false,
                size: 0,
                files: 0,
            });
            items.len() - 1
        });
        let item = &mut items[i];
        item.dir |= e.dir || nested;
        if !e.dir {
            item.size += e.size;
            item.files += 1;
        }
    }
    items
}

/// A file or folder to send, with where it is on this computer.
struct Source {
    entry: Entry,
    path: PathBuf,
}

/// List everything to send. Folders are walked; symbolic links, system
/// files and names Windows cannot store are skipped and counted.
fn scan(paths: &[PathBuf], cancel: &AtomicBool) -> Result<(Vec<Source>, usize)> {
    let mut out = vec![];
    let mut skipped = 0;
    let mut tops = HashSet::new();
    for path in paths {
        cancelled(cancel)?;
        let shown = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        // The link itself is not sent, so its target must not be followed
        // here either; walk() skips links the same way.
        let meta = fs::symlink_metadata(path)
            .map_err(|e| fail(format!("无法读取「{shown}」：{}", describe(&e))))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| safe_name(n))
            .ok_or_else(|| {
                fail(format!(
                    "「{shown}」的名称包含 Windows 不支持的字符或过长，请改名后再发送"
                ))
            })?;
        let mut top = name.to_owned();
        let mut n = 2;
        while !tops.insert(top.to_lowercase()) {
            top = numbered(name, n, meta.is_dir());
            n += 1;
        }
        if meta.is_dir() {
            out.push(Source {
                entry: Entry {
                    path: top.clone(),
                    size: 0,
                    dir: true,
                },
                path: path.clone(),
            });
            walk(path, &top, 2, &mut out, &mut skipped, cancel)?;
        } else if meta.is_file() {
            if meta.len() > MAX_FILE {
                return Err(fail(format!("「{name}」超过 100 GB，无法发送")));
            }
            out.push(Source {
                entry: Entry {
                    path: top,
                    size: meta.len(),
                    dir: false,
                },
                path: path.clone(),
            });
        } else {
            skipped += 1;
        }
        check_count(out.len())?;
    }
    if out.is_empty() {
        return Err(fail("没有可以发送的文件"));
    }
    Ok((out, skipped))
}

fn check_count(n: usize) -> Result<()> {
    if n > MAX_ENTRIES {
        Err(fail(format!(
            "一次最多发送 {MAX_ENTRIES} 个文件和文件夹，请分批发送或先压缩"
        )))
    } else {
        Ok(())
    }
}

fn walk(
    dir: &Path,
    relative: &str,
    depth: usize,
    out: &mut Vec<Source>,
    skipped: &mut usize,
    cancel: &AtomicBool,
) -> Result<()> {
    let Ok(read) = fs::read_dir(dir) else {
        *skipped += 1;
        return Ok(());
    };
    let mut children: Vec<_> = read.filter_map(|e| e.ok()).collect();
    children.sort_by_key(|e| e.file_name());
    let mut names = HashSet::new();
    for child in children {
        cancelled(cancel)?;
        let Some(name) = child.file_name().to_str().map(str::to_owned) else {
            *skipped += 1;
            continue;
        };
        if is_junk(&name) {
            continue;
        }
        let path = format!("{relative}/{name}");
        let Ok(kind) = child.file_type() else {
            *skipped += 1;
            continue;
        };
        if kind.is_symlink()
            || !safe_name(&name)
            || path.len() > MAX_PATH_BYTES
            || depth > MAX_DEPTH
            || !names.insert(name.to_lowercase())
        {
            *skipped += 1;
            continue;
        }
        if kind.is_dir() {
            out.push(Source {
                entry: Entry {
                    path: path.clone(),
                    size: 0,
                    dir: true,
                },
                path: child.path(),
            });
            walk(&child.path(), &path, depth + 1, out, skipped, cancel)?;
        } else if kind.is_file() {
            match child.metadata() {
                Ok(m) if m.len() <= MAX_FILE => out.push(Source {
                    entry: Entry {
                        path,
                        size: m.len(),
                        dir: false,
                    },
                    path: child.path(),
                }),
                _ => *skipped += 1,
            }
        } else {
            *skipped += 1;
        }
        check_count(out.len())?;
    }
    Ok(())
}

// ───────────────────────────── connections ─────────────────────────────

fn configure(s: &TcpStream, timeout: Duration) -> Result<()> {
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    s.set_nodelay(true)?;
    Ok(())
}

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(ended(Stage::Cancelled, "已取消"))
    } else {
        Ok(())
    }
}

/// Stops the cancel watcher when dropped.
struct CancelWatch(Arc<AtomicBool>);
impl Drop for CancelWatch {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Shut the socket down as soon as `cancel` is set, so a blocked read or write
/// (waiting for approval, a stalled network) returns at once and temporary
/// files are cleaned up.
fn watch_cancel(stream: &TcpStream, cancel: &Arc<AtomicBool>) -> io::Result<CancelWatch> {
    let watched = stream.try_clone()?;
    let stop = Arc::new(AtomicBool::new(false));
    let (stop_flag, cancel) = (stop.clone(), cancel.clone());
    std::thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            if cancel.load(Ordering::Relaxed) {
                let _ = watched.shutdown(std::net::Shutdown::Both);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    Ok(CancelWatch(stop))
}

enum Decision {
    Accepted { trust: bool },
    Rejected,
    TimedOut,
    SenderLeft,
}

/// Wait for the user's answer while watching the connection, so a request the
/// sender has cancelled disappears instead of lingering until the timeout.
fn await_decision(s: &TcpStream, rx: &mpsc::Receiver<Answer>) -> Decision {
    let start = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(a) if a.accept => return Decision::Accepted { trust: a.trust },
            Ok(_) | Err(RecvTimeoutError::Disconnected) => return Decision::Rejected,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if start.elapsed() >= CONSENT_TIMEOUT {
            return Decision::TimedOut;
        }
        if peer_closed(s) {
            return Decision::SenderLeft;
        }
    }
}

/// True when the other side has closed or reset the connection. The sender
/// sends nothing while it waits for approval, so readable data means EOF.
fn peer_closed(s: &TcpStream) -> bool {
    if s.set_nonblocking(true).is_err() {
        return false;
    }
    let closed = match s.peek(&mut [0; 1]) {
        Ok(0) => true,
        Ok(_) => false,
        Err(e) => e.kind() != io::ErrorKind::WouldBlock,
    };
    let _ = s.set_nonblocking(false);
    closed
}

fn about(shared: &Shared) -> About {
    About {
        name: shared.settings.lock().unwrap().name.clone(),
        platform: Platform::this(),
    }
}

/// Keep a trusted device's name and last address current.
fn remember(shared: &Shared, remote: &Remote, ip: IpAddr) {
    let _ = shared.change_settings(|s| {
        if let Some(t) = s.trusted.iter_mut().find(|t| t.id == remote.id) {
            t.name = remote.about.name.clone();
            t.platform = remote.about.platform;
            t.address = ip.to_string();
        }
    });
}

/// Receive from this device without asking from now on.
pub fn trust(shared: &Shared, id: &str, name: &str, platform: Platform, address: &str) {
    let result = shared.change_settings(|s| {
        if !s.is_trusted(id) {
            s.trusted.push(TrustedDevice {
                id: id.to_owned(),
                name: name.to_owned(),
                platform,
                address: address.to_owned(),
            });
        }
    });
    if let Err(e) = result {
        shared.event(Event::Note(format!("无法保存信任设置：{e}")));
    }
}

/// Bytes and speed shown while a batch is transferred.
struct Progress<'a> {
    shared: &'a Shared,
    id: u64,
    done: u64,
    files: usize,
    current: String,
    tick: Instant,
    tick_done: u64,
    rate: f64,
}

impl<'a> Progress<'a> {
    fn new(shared: &'a Shared, id: u64) -> Self {
        Self {
            shared,
            id,
            done: 0,
            files: 0,
            current: String::new(),
            tick: Instant::now(),
            tick_done: 0,
            rate: 0.,
        }
    }
    fn file(&mut self, name: &str) {
        self.current = name.rsplit('/').next().unwrap_or(name).to_owned();
    }
    fn add(&mut self, n: usize) {
        self.done += n as u64;
        if self.tick.elapsed() >= Duration::from_millis(200) {
            self.publish();
        }
    }
    fn file_done(&mut self) {
        self.files += 1;
    }
    fn publish(&mut self) {
        let elapsed = self.tick.elapsed().as_secs_f64().max(0.001);
        let now = (self.done - self.tick_done) as f64 / elapsed;
        self.rate = if self.rate == 0. {
            now
        } else {
            self.rate * 0.7 + now * 0.3
        };
        self.tick = Instant::now();
        self.tick_done = self.done;
        let (done, files, rate, current) = (self.done, self.files, self.rate, self.current.clone());
        self.shared.update(self.id, |t| {
            t.done = done;
            t.files_done = files;
            t.rate = rate;
            t.current = current;
        });
    }
}

/// Final state of a transfer from its outcome.
fn conclude(t: &mut Transfer, outcome: &Result<()>, cancel: &AtomicBool) {
    match outcome {
        Ok(()) => {
            t.stage = Stage::Done;
            t.done = t.total;
            t.files_done = t.files;
            t.rate = 0.;
        }
        Err(e) => {
            if cancel.load(Ordering::Relaxed) {
                t.stage = Stage::Cancelled;
                t.detail = "已取消".into();
            } else if let Some(Ended(stage, text)) = e.downcast_ref::<Ended>() {
                t.stage = *stage;
                t.detail = text.clone();
            } else {
                t.stage = Stage::Failed;
                t.detail = describe(e.as_ref());
            }
        }
    }
}

// ───────────────────────────── receiving ─────────────────────────────

pub fn start_receiver(shared: Arc<Shared>) {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
            Ok(v) => v,
            Err(e) => {
                *shared.receiver_note.lock().unwrap() = Some(format!(
                    "无法接收文件：端口 {PORT} 被占用（{e}）。请关闭占用该端口的程序后重新打开邻传"
                ));
                shared.wake();
                return;
            }
        };
        shared.ready.store(true, Ordering::Relaxed);
        shared.wake();
        serve(listener, shared);
    });
}

/// Accept connections on `listener` until the process ends.
pub fn serve(listener: TcpListener, shared: Arc<Shared>) {
    let count = Arc::new(AtomicUsize::new(0));
    let per_ip: Arc<Mutex<HashMap<IpAddr, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    for s in listener.incoming() {
        let Ok(s) = s else {
            // A listener stuck on errors (out of file descriptors) must not
            // spin at full speed.
            std::thread::sleep(Duration::from_millis(200));
            continue;
        };
        let Ok(addr) = s.peer_addr() else {
            continue;
        };
        if count.fetch_add(1, Ordering::Relaxed) >= MAX_CONNECTIONS {
            count.fetch_sub(1, Ordering::Relaxed);
            continue;
        }
        {
            let mut per_ip = per_ip.lock().unwrap();
            if !admit(&mut per_ip, addr.ip(), MAX_PER_IP) {
                drop(per_ip);
                count.fetch_sub(1, Ordering::Relaxed);
                continue;
            }
        }
        let shared = shared.clone();
        let count = count.clone();
        let per_ip = per_ip.clone();
        std::thread::spawn(move || {
            let _ = handle(s, &shared);
            count.fetch_sub(1, Ordering::Relaxed);
            release(&mut per_ip.lock().unwrap(), addr.ip());
        });
    }
}

/// Count a connection from `ip` against its per-address limit.
fn admit(per_ip: &mut HashMap<IpAddr, usize>, ip: IpAddr, limit: usize) -> bool {
    let n = per_ip.entry(ip).or_insert(0);
    if *n >= limit {
        false
    } else {
        *n += 1;
        true
    }
}

fn release(per_ip: &mut HashMap<IpAddr, usize>, ip: IpAddr) {
    if let Some(n) = per_ip.get_mut(&ip) {
        *n -= 1;
        if *n == 0 {
            per_ip.remove(&ip);
        }
    }
}

fn handle(stream: TcpStream, shared: &Arc<Shared>) -> Result<()> {
    // The handshake and request must arrive promptly, so idle connections
    // cannot hold the connection slots.
    configure(&stream, Duration::from_secs(10))?;
    let ip = stream.peer_addr()?.ip();
    let (mut ch, remote) = wire::accept(stream, &shared.identity, &about(shared))?;
    let offer: Offer = ch.recv_json(MAX_OFFER)?;
    let settings = shared.settings.lock().unwrap().clone();
    let trusted = settings.is_trusted(&remote.id);
    if trusted {
        remember(shared, &remote, ip);
    }
    let refusal = if !settings.receive {
        Some("对方已关闭接收")
    } else if settings.trusted_only && !trusted {
        Some("对方只接收已信任设备发来的内容")
    } else {
        None
    };
    if let Some(reason) = refusal {
        ch.send_json(&Reply {
            accept: false,
            reason: reason.into(),
        })?;
        return Ok(());
    }
    match offer {
        Offer::Text { text } => receive_text(ch, shared, &remote, ip, text),
        Offer::Files { entries } => {
            receive_files(ch, shared, &remote, ip, entries, trusted, &settings.folder)
        }
    }
}

fn receive_text(
    mut ch: Secure,
    shared: &Shared,
    remote: &Remote,
    ip: IpAddr,
    text: String,
) -> Result<()> {
    if text.trim().is_empty() || text.len() > MAX_TEXT {
        ch.send_json(&Reply {
            accept: false,
            reason: "文字为空或过长".into(),
        })?;
        return Err(fail("文字为空或过长"));
    }
    if shared.messages.lock().unwrap().len() >= MAX_PENDING_TEXTS {
        ch.send_json(&Reply {
            accept: false,
            reason: "对方有太多未读的文字".into(),
        })?;
        return Err(fail("未读文字过多，已拒收"));
    }
    ch.send_json(&Reply {
        accept: true,
        reason: String::new(),
    })?;
    let mut t = Transfer::new(false, &remote.about.name, ip.to_string());
    t.text = Some(text.clone());
    t.total = text.len() as u64;
    t.done = t.total;
    t.stage = Stage::Done;
    let (id, _) = shared.add(t);
    shared.messages.lock().unwrap().push(Message {
        id,
        peer: remote.about.name.clone(),
        text,
    });
    shared.show();
    Ok(())
}

fn receive_files(
    mut ch: Secure,
    shared: &Shared,
    remote: &Remote,
    ip: IpAddr,
    entries: Vec<Entry>,
    trusted: bool,
    folder: &Path,
) -> Result<()> {
    if let Err(e) = validate(&entries) {
        let _ = ch.send_json(&Reply {
            accept: false,
            reason: describe(e.as_ref()),
        });
        return Err(e);
    }
    let items = summarize(&entries);
    let mut t = Transfer::new(false, &remote.about.name, ip.to_string());
    t.items = items.iter().map(|i| i.name.clone()).collect();
    t.files = entries.iter().filter(|e| !e.dir).count();
    t.total = entries.iter().map(|e| e.size).sum();
    t.stage = Stage::Waiting;
    let (files, total) = (t.files, t.total);
    let (id, cancel) = shared.add(t);
    let _watch = watch_cancel(ch.stream(), &cancel)?;
    let mut saved = vec![];
    let mut accepted = false;
    let outcome = (|| -> Result<()> {
        if !trusted {
            let (tx, rx) = mpsc::sync_channel(1);
            shared.requests.lock().unwrap().push(Request {
                id,
                peer: remote.about.name.clone(),
                peer_id: remote.id.clone(),
                platform: remote.about.platform,
                address: ip.to_string(),
                items,
                files,
                total,
                created: Instant::now(),
                decision: tx,
            });
            shared.show();
            let decision = await_decision(ch.stream(), &rx);
            shared.requests.lock().unwrap().retain(|r| r.id != id);
            shared.wake();
            match decision {
                Decision::Accepted { trust: true } => trust(
                    shared,
                    &remote.id,
                    &remote.about.name,
                    remote.about.platform,
                    &ip.to_string(),
                ),
                Decision::Accepted { trust: false } => {}
                Decision::SenderLeft => return Err(ended(Stage::Cancelled, "对方已取消发送")),
                Decision::Rejected => {
                    let _ = ch.send_json(&Reply {
                        accept: false,
                        reason: "对方拒绝了接收".into(),
                    });
                    return Err(ended(Stage::Declined, "已拒绝"));
                }
                Decision::TimedOut => {
                    let _ = ch.send_json(&Reply {
                        accept: false,
                        reason: "对方没有在两分钟内确认".into(),
                    });
                    return Err(ended(Stage::Declined, "超时未确认，已自动拒绝"));
                }
            }
        }
        cancelled(&cancel)?;
        if let Err(e) = fs::create_dir_all(folder) {
            let _ = ch.send_json(&Reply {
                accept: false,
                reason: "对方无法写入接收文件夹".into(),
            });
            return Err(fail(format!("无法创建接收文件夹：{}", describe(&e))));
        }
        ch.send_json(&Reply {
            accept: true,
            reason: String::new(),
        })?;
        accepted = true;
        shared.update(id, |t| t.stage = Stage::Running);
        configure(ch.stream(), Duration::from_secs(30))?;
        receive_entries(&mut ch, shared, id, &cancel, folder, &entries, &mut saved)?;
        ch.send_json(&Receipt {
            ok: true,
            reason: String::new(),
        })?;
        Ok(())
    })();
    // After the accept reply, tell the sender why the batch failed; until
    // then the decline itself is the answer. Best effort: the socket may
    // already be gone (the sender quit or cancelled).
    if accepted {
        if let Err(e) = &outcome {
            let reason = if cancel.load(Ordering::Relaxed) {
                "对方已取消接收".to_owned()
            } else {
                describe(e.as_ref())
            };
            let _ = ch.send_json(&Receipt { ok: false, reason });
        }
    }
    shared.update(id, |t| {
        t.saved = saved.clone();
        conclude(t, &outcome, &cancel);
        if t.stage == Stage::Failed && !saved.is_empty() {
            t.detail = format!("{}（已保存的部分可以打开）", t.detail);
        }
    });
    match &outcome {
        Ok(()) => shared.event(Event::Received { id }),
        Err(e) if !e.is::<Ended>() && !cancel.load(Ordering::Relaxed) => {
            shared.event(Event::Failed { id })
        }
        Err(_) => {}
    }
    outcome
}

fn receive_entries(
    ch: &mut Secure,
    shared: &Shared,
    id: u64,
    cancel: &AtomicBool,
    root: &Path,
    entries: &[Entry],
    saved: &mut Vec<PathBuf>,
) -> Result<()> {
    // Top-level folders get a free name in the receive folder; everything
    // inside them lands in the new folder, so it cannot collide.
    let mut folders: HashMap<&str, PathBuf> = HashMap::new();
    let mut progress = Progress::new(shared, id);
    for e in entries {
        cancelled(cancel)?;
        let (top, rest) = match e.path.split_once('/') {
            Some((top, rest)) => (top, Some(rest)),
            None => (e.path.as_str(), None),
        };
        if !e.dir && rest.is_none() {
            progress.file(&e.path);
            let temp = receive_file(ch, e.size, root, cancel, &mut progress)?;
            saved.push(persist_free(temp, root, top)?);
            progress.file_done();
            continue;
        }
        let base = match folders.get(top) {
            Some(p) => p.clone(),
            None => {
                let p = claim_folder(root, top)?;
                saved.push(p.clone());
                folders.insert(top, p.clone());
                p
            }
        };
        let dest = match rest {
            Some(r) => base.join(native(r)),
            None => base,
        };
        if e.dir {
            fs::create_dir_all(&dest)?;
            continue;
        }
        let parent = dest.parent().ok_or_else(|| fail("路径无效"))?;
        fs::create_dir_all(parent)?;
        progress.file(&e.path);
        let temp = receive_file(ch, e.size, parent, cancel, &mut progress)?;
        temp.persist_noclobber(&dest).map_err(|e| e.error)?;
        progress.file_done();
    }
    progress.publish();
    Ok(())
}

fn receive_file(
    ch: &mut Secure,
    size: u64,
    dir: &Path,
    cancel: &AtomicBool,
    progress: &mut Progress,
) -> Result<tempfile::NamedTempFile> {
    let mut temp = tempfile::Builder::new()
        .prefix(".lantransfer-")
        .tempfile_in(dir)?;
    let mut hash = Sha256::new();
    let mut done = 0;
    while done < size {
        cancelled(cancel)?;
        let data = ch.recv()?;
        if data.is_empty() || data.len() as u64 > size - done {
            return Err(fail("文件长度异常"));
        }
        temp.write_all(data)?;
        hash.update(data);
        done += data.len() as u64;
        progress.add(data.len());
    }
    cancelled(cancel)?;
    if ch.recv()? != hash.finalize().as_slice() {
        return Err(fail("文件校验失败：内容在传输中损坏"));
    }
    Ok(temp)
}

/// Save a received top-level file under its name, or "name (2)" and so on
/// when that is taken. Never overwrites.
fn persist_free(mut temp: tempfile::NamedTempFile, root: &Path, name: &str) -> Result<PathBuf> {
    for n in 1..=9999 {
        let candidate = if n == 1 {
            name.to_owned()
        } else {
            numbered(name, n, false)
        };
        let dest = root.join(&candidate);
        match temp.persist_noclobber(&dest) {
            Ok(_) => return Ok(dest),
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => temp = e.file,
            Err(e) => return Err(e.error.into()),
        }
    }
    Err(fail("同名文件过多"))
}

/// Create a new top-level folder, numbered when the name is taken.
fn claim_folder(root: &Path, name: &str) -> Result<PathBuf> {
    for n in 1..=9999 {
        let candidate = if n == 1 {
            name.to_owned()
        } else {
            numbered(name, n, true)
        };
        let dest = root.join(&candidate);
        match fs::create_dir(&dest) {
            Ok(()) => return Ok(dest),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(fail("同名文件夹过多"))
}

// ───────────────────────────── sending ─────────────────────────────

pub enum Payload {
    Files(Vec<PathBuf>),
    Text(String),
}

/// Where to send: a discovered device (with its expected identity) or an
/// address typed in by hand (empty `id`).
#[derive(Clone, Debug)]
pub struct Target {
    pub address: SocketAddr,
    pub id: String,
    pub name: String,
}

/// Start sending in the background; returns the transfer's ID.
pub fn send(shared: Arc<Shared>, target: Target, payload: Payload) -> u64 {
    let mut t = Transfer::new(true, &target.name, target.address.ip().to_string());
    t.target = Some((target.address, target.id.clone()));
    match &payload {
        Payload::Files(paths) => {
            t.sources = paths.clone();
            t.items = paths
                .iter()
                .map(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
                .collect();
            t.stage = Stage::Preparing;
        }
        Payload::Text(text) => {
            t.text = Some(text.clone());
            t.total = text.len() as u64;
        }
    }
    let (id, cancel) = shared.add(t);
    std::thread::spawn(move || {
        let outcome = send_batch(&shared, id, &cancel, &target, payload);
        shared.update(id, |t| conclude(t, &outcome, &cancel));
        match &outcome {
            Ok(()) => shared.event(Event::Sent { id }),
            Err(e) if !e.is::<Ended>() && !cancel.load(Ordering::Relaxed) => {
                shared.event(Event::Failed { id })
            }
            Err(_) => {}
        }
    });
    id
}

fn send_batch(
    shared: &Arc<Shared>,
    id: u64,
    cancel: &Arc<AtomicBool>,
    target: &Target,
    payload: Payload,
) -> Result<()> {
    let (offer, sources) = match payload {
        Payload::Text(text) => {
            if text.trim().is_empty() {
                return Err(fail("请输入要发送的文字"));
            }
            if text.len() > MAX_TEXT {
                return Err(fail("文字过长，一次最多发送 256 KB"));
            }
            (Offer::Text { text }, vec![])
        }
        Payload::Files(paths) => {
            let (sources, skipped) = scan(&paths, cancel)?;
            let items = sources
                .iter()
                .filter(|s| !s.entry.path.contains('/'))
                .map(|s| s.entry.path.clone())
                .collect();
            let files = sources.iter().filter(|s| !s.entry.dir).count();
            let total = sources.iter().map(|s| s.entry.size).sum();
            shared.update(id, |t| {
                t.items = items;
                t.files = files;
                t.total = total;
                if skipped > 0 {
                    t.detail = format!("已跳过 {skipped} 个无法发送的项目（快捷方式或特殊文件名）");
                }
            });
            let entries = sources.iter().map(|s| s.entry.clone()).collect();
            (Offer::Files { entries }, sources)
        }
    };
    cancelled(cancel)?;
    shared.update(id, |t| t.stage = Stage::Connecting);
    let stream = TcpStream::connect_timeout(&target.address, Duration::from_secs(5))?;
    configure(&stream, Duration::from_secs(15))?;
    let _watch = watch_cancel(&stream, cancel)?;
    let (mut ch, remote) = wire::connect(stream, &shared.identity, &about(shared))?;
    if !target.id.is_empty() && remote.id != target.id {
        return Err(fail("对方的设备身份与列表中的不一致，为安全起见已停止发送"));
    }
    remember(shared, &remote, target.address.ip());
    shared.update(id, |t| {
        t.peer = remote.about.name.clone();
        t.stage = Stage::Waiting;
    });
    ch.send_json(&offer)?;
    // The receiver may take up to CONSENT_TIMEOUT to answer.
    ch.stream()
        .set_read_timeout(Some(CONSENT_TIMEOUT + Duration::from_secs(15)))?;
    let reply: Reply = ch.recv_json(64 * 1024)?;
    if !reply.accept {
        let reason = if reply.reason.is_empty() {
            "对方拒绝了接收".to_owned()
        } else {
            wire::clean_name(&reply.reason)
        };
        return Err(ended(Stage::Declined, reason));
    }
    if sources.is_empty() {
        return Ok(());
    }
    configure(ch.stream(), Duration::from_secs(30))?;
    shared.update(id, |t| t.stage = Stage::Running);
    let mut progress = Progress::new(shared, id);
    let mut buf = vec![0; CHUNK];
    for s in sources.iter().filter(|s| !s.entry.dir) {
        cancelled(cancel)?;
        progress.file(&s.entry.path);
        let changed = || {
            fail(format!(
                "「{}」在发送期间被修改或删除，请重新发送",
                s.entry.path.rsplit('/').next().unwrap_or_default()
            ))
        };
        let mut file = File::open(&s.path).map_err(|_| changed())?;
        if file.metadata()?.len() != s.entry.size {
            return Err(changed());
        }
        let mut hash = Sha256::new();
        let mut left = s.entry.size;
        while left > 0 {
            cancelled(cancel)?;
            let want = (left as usize).min(CHUNK);
            let n = file.read(&mut buf[..want])?;
            if n == 0 {
                return Err(changed());
            }
            ch.send(&buf[..n])?;
            hash.update(&buf[..n]);
            left -= n as u64;
            progress.add(n);
        }
        ch.send(&hash.finalize())?;
        progress.file_done();
    }
    progress.publish();
    // The receiver answers after saving the last file.
    ch.stream()
        .set_read_timeout(Some(Duration::from_secs(120)))?;
    let receipt: Receipt = ch.recv_json(64 * 1024)?;
    if !receipt.ok {
        return Err(fail(wire::clean_name(&receipt.reason)));
    }
    Ok(())
}

/// IPv4 addresses of this computer, private networks first.
pub fn local_addresses() -> Vec<Ipv4Addr> {
    let mut list: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|x| match x.addr {
            if_addrs::IfAddr::V4(v) if !v.ip.is_loopback() && !v.ip.is_link_local() => Some(v.ip),
            _ => None,
        })
        .collect();
    list.sort_by_key(|ip| (!ip.is_private(), *ip));
    list.dedup();
    list
}

pub fn parse_address(text: &str) -> std::result::Result<SocketAddr, String> {
    let text = text.trim();
    if let Ok(ip) = text.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, PORT));
    }
    text.parse()
        .map_err(|_| "请输入有效的 IP 地址，例如 192.168.1.20".into())
}

#[cfg(test)]
#[path = "../tests/support/network_cases.rs"]
mod tests;
