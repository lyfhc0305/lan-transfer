//! Settings, shared state between the interface and the network threads, and
//! small formatting helpers.
use eframe::egui;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicIsize},
        mpsc::SyncSender,
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// TCP port for transfers and UDP port for discovery.
pub const PORT: u16 = 45873;
/// How long an incoming request waits for the user to accept it.
pub const CONSENT_TIMEOUT: Duration = Duration::from_secs(110);
/// Transfers kept in the list (finished ones are dropped oldest first).
const MAX_TRANSFERS: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Mac,
    Windows,
    Linux,
    #[default]
    #[serde(other)]
    Other,
}

impl Platform {
    pub const fn this() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Mac => "Mac",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::Other => "电脑",
        }
    }
}

/// A device whose files are received without asking.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TrustedDevice {
    /// Public key (hex), see [`Identity`].
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub platform: Platform,
    /// Last address it was seen at; probed directly when broadcasts are blocked.
    #[serde(default)]
    pub address: String,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    /// X25519 private key (hex) that identifies this computer.
    pub identity: String,
    pub folder: PathBuf,
    pub receive: bool,
    /// Decline requests from devices that are not trusted.
    pub trusted_only: bool,
    pub close_to_tray: bool,
    pub trusted: Vec<TrustedDevice>,
    /// The one-time notice that closing the window keeps the app running
    /// has been shown.
    pub tray_hint_shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let folder = directories::UserDirs::new()
            .map(|d| d.download_dir().unwrap_or(d.home_dir()).join("邻传接收"))
            .unwrap_or_else(|| PathBuf::from("received"));
        Self {
            name: String::new(),
            identity: String::new(),
            folder,
            receive: true,
            trusted_only: false,
            close_to_tray: true,
            trusted: vec![],
            tray_hint_shown: false,
        }
    }
}

impl Settings {
    pub fn is_trusted(&self, id: &str) -> bool {
        self.trusted.iter().any(|t| t.id == id)
    }
    /// Fill in a generated identity and the computer's name where missing.
    pub(crate) fn complete(&mut self) {
        if Identity::from_hex(&self.identity).is_none() {
            self.identity = Identity::generate().private_hex();
        }
        if self.name.trim().is_empty() {
            self.name = default_device_name();
        }
    }
}

/// The computer's name as shown in Finder / Explorer, used as the default
/// device name so both sides recognise each other.
pub fn default_device_name() -> String {
    let found = if cfg!(target_os = "macos") {
        std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "ComputerName"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    } else if cfg!(target_os = "windows") {
        std::env::var("COMPUTERNAME").ok()
    } else {
        fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_owned())
    };
    found
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().filter(|c| !c.is_control()).take(40).collect())
        .unwrap_or_else(|| format!("我的 {}", Platform::this().label()))
}

/// Long-term key pair of this computer. The public key is the device ID that
/// other computers remember when they trust this one.
#[derive(Clone)]
pub struct Identity {
    pub private: [u8; 32],
    pub public: [u8; 32],
}

impl Identity {
    pub fn generate() -> Self {
        let mut private = [0; 32];
        OsRng.fill_bytes(&mut private);
        Self::from_private(private)
    }
    pub fn from_private(private: [u8; 32]) -> Self {
        use snow::{params::DHChoice, resolvers::CryptoResolver};
        let mut dh = snow::resolvers::DefaultResolver
            .resolve_dh(&DHChoice::Curve25519)
            .expect("X25519 is built in");
        dh.set(&private);
        let mut public = [0; 32];
        public.copy_from_slice(dh.pubkey());
        Self { private, public }
    }
    pub fn from_hex(text: &str) -> Option<Self> {
        let bytes = from_hex(text.trim())?;
        Some(Self::from_private(bytes.try_into().ok()?))
    }
    pub fn private_hex(&self) -> String {
        to_hex(&self.private)
    }
    pub fn id(&self) -> String {
        to_hex(&self.public)
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() & 1 == 1 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// Short, human-comparable form of a device ID, e.g. "7F3A 91C2 D04B".
pub fn fingerprint(id: &str) -> String {
    let digest = Sha256::digest(id.as_bytes());
    let hex = to_hex(&digest[..6]).to_uppercase();
    format!("{} {} {}", &hex[0..4], &hex[4..8], &hex[8..12])
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LAN_TRANSFER_CONFIG_DIR") {
        return dir.into();
    }
    directories::ProjectDirs::from("app", "LanTransfer", "LanTransfer")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Load settings; the second value is a warning to show when they could not
/// be read. New identities are saved right away so the device ID is stable.
pub fn load_settings() -> (Settings, Option<String>) {
    let (mut settings, mut warning) = match fs::read(config_path()) {
        Ok(data) => match serde_json::from_slice::<Settings>(&data) {
            Ok(s) => (s, None),
            Err(_) => (
                Settings::default(),
                Some("原设置无法读取，已恢复默认设置。".to_owned()),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
        Err(e) => (Settings::default(), Some(format!("读取设置失败：{e}"))),
    };
    let before = settings.clone();
    settings.complete();
    if settings != before {
        if let Err(e) = save_settings(&settings) {
            warning.get_or_insert(format!("无法保存设置：{e}"));
        }
    }
    (settings, warning)
}

pub fn save_settings(s: &Settings) -> Result<(), String> {
    if Identity::from_hex(&s.identity).is_none() {
        return Err("设备密钥无效".into());
    }
    let p = config_path();
    let dir = p.parent().ok_or("设置路径不可用")?;
    fs::create_dir_all(dir).map_err(|e| format!("无法创建设置目录：{e}"))?;
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(&p).map_err(|e| e.to_string())?;
    Ok(())
}

/// A computer found on the network (or typed in by address).
#[derive(Clone, Debug)]
pub struct Peer {
    /// Public key (hex); empty for an address typed in that has not answered.
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub address: SocketAddr,
    pub seen: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    /// Listing folders before sending.
    Preparing,
    Connecting,
    /// The receiver has not answered the request yet.
    Waiting,
    Running,
    Done,
    Failed,
    /// Declined or not answered in time.
    Declined,
    Cancelled,
}

impl Stage {
    pub fn finished(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Declined | Self::Cancelled
        )
    }
}

/// One batch of files (or one text message) sent or received.
#[derive(Clone, Debug)]
pub struct Transfer {
    pub id: u64,
    pub outgoing: bool,
    /// Set for text messages.
    pub text: Option<String>,
    /// The other computer's name.
    pub peer: String,
    pub address: String,
    /// Names of the items the user picked (top level only).
    pub items: Vec<String>,
    pub files: usize,
    pub total: u64,
    pub done: u64,
    pub files_done: usize,
    /// File currently being transferred.
    pub current: String,
    /// Bytes per second, smoothed; see [`Transfer::speed`].
    pub rate: f64,
    /// When progress was last reported, so a stalled transfer shows no speed.
    pub updated: Instant,
    pub stage: Stage,
    /// Failure reason or other note.
    pub detail: String,
    /// Received items as saved (top level), for "open" / "show in folder".
    pub saved: Vec<PathBuf>,
    /// What was picked to send and where to, so a failed batch can be retried.
    pub sources: Vec<PathBuf>,
    pub target: Option<(SocketAddr, String)>,
    pub started: Instant,
    pub ended: Option<Instant>,
    pub cancel: Arc<AtomicBool>,
}

impl Transfer {
    pub fn new(outgoing: bool, peer: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            id: OsRng.next_u64(),
            outgoing,
            text: None,
            peer: peer.into(),
            address: address.into(),
            items: vec![],
            files: 0,
            total: 0,
            done: 0,
            files_done: 0,
            current: String::new(),
            rate: 0.,
            updated: Instant::now(),
            stage: Stage::Connecting,
            detail: String::new(),
            saved: vec![],
            sources: vec![],
            target: None,
            started: Instant::now(),
            ended: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    /// "报告.pdf", "照片", "报告.pdf 等 3 项" or the start of a text message.
    pub fn title(&self) -> String {
        if let Some(text) = &self.text {
            let line: String = text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(60)
                .collect();
            return if line.is_empty() {
                "文字".into()
            } else {
                line
            };
        }
        match self.items.as_slice() {
            [] => "文件".into(),
            [one] => one.clone(),
            [first, ..] => format!("{first} 等 {} 项", self.items.len()),
        }
    }
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            if self.stage == Stage::Done {
                1.
            } else {
                0.
            }
        } else {
            (self.done as f64 / self.total as f64).clamp(0., 1.) as f32
        }
    }
    /// Current speed in bytes per second; 0 once no data has arrived for
    /// a couple of seconds.
    pub fn speed(&self) -> f64 {
        if self.stage == Stage::Running && self.updated.elapsed() < Duration::from_secs(2) {
            self.rate
        } else {
            0.
        }
    }
    /// Remaining time at the current speed.
    pub fn eta(&self) -> Option<Duration> {
        let speed = self.speed();
        (speed > 1.)
            .then(|| Duration::from_secs_f64(self.total.saturating_sub(self.done) as f64 / speed))
    }
}

/// Summary of one top-level item in an incoming request.
#[derive(Clone, Debug)]
pub struct ItemSummary {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub files: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Answer {
    pub accept: bool,
    /// Receive from this device without asking from now on.
    pub trust: bool,
}

/// An incoming batch (or text) waiting for the user to accept it.
#[derive(Clone)]
pub struct Request {
    /// Same as the matching [`Transfer::id`].
    pub id: u64,
    pub peer: String,
    pub peer_id: String,
    pub platform: Platform,
    pub address: String,
    /// Set for a text message; `items` is then empty.
    pub text: Option<String>,
    pub items: Vec<ItemSummary>,
    pub files: usize,
    pub total: u64,
    /// Free space on the receive folder's disk, when known.
    pub free: Option<u64>,
    pub created: Instant,
    pub decision: SyncSender<Answer>,
}

/// A text message waiting to be read.
#[derive(Clone, Debug)]
pub struct Message {
    pub id: u64,
    pub peer: String,
    pub text: String,
}

/// Something that happened in the background, shown once as a notice.
#[derive(Clone, Debug)]
pub enum Event {
    Received { id: u64 },
    Sent { id: u64 },
    Failed { id: u64 },
    Note(String),
}

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub identity: Identity,
    pub peers: Mutex<Vec<Peer>>,
    pub transfers: Mutex<Vec<Transfer>>,
    pub requests: Mutex<Vec<Request>>,
    pub messages: Mutex<Vec<Message>>,
    pub events: Mutex<Vec<Event>>,
    /// Lasting problem with receiving (port in use).
    pub receiver_note: Mutex<Option<String>>,
    /// Lasting problem with discovery (UDP port in use).
    pub discovery_note: Mutex<Option<String>>,
    pub ready: AtomicBool,
    /// Discovery socket, used to probe an address typed in by the user.
    pub beacon: Mutex<Option<std::net::UdpSocket>>,
    /// Addresses typed in by the user that are probed with the broadcasts.
    pub manual: Mutex<Vec<std::net::IpAddr>>,
    /// Native window handle (Win32 HWND), 0 when unknown. See [`Shared::show`].
    pub window: AtomicIsize,
    /// False while the window is hidden in the menu bar / tray.
    pub visible: AtomicBool,
    pub ctx: egui::Context,
}

impl Shared {
    pub fn new(settings: Settings, ctx: egui::Context) -> Arc<Self> {
        let mut settings = settings;
        settings.complete();
        let identity = Identity::from_hex(&settings.identity).expect("completed above");
        Arc::new(Self {
            settings: Mutex::new(settings),
            identity,
            peers: Mutex::new(vec![]),
            transfers: Mutex::new(vec![]),
            requests: Mutex::new(vec![]),
            messages: Mutex::new(vec![]),
            events: Mutex::new(vec![]),
            receiver_note: Mutex::new(None),
            discovery_note: Mutex::new(None),
            ready: AtomicBool::new(false),
            beacon: Mutex::new(None),
            manual: Mutex::new(vec![]),
            window: AtomicIsize::new(0),
            visible: AtomicBool::new(true),
            ctx,
        })
    }
    pub fn id(&self) -> String {
        self.identity.id()
    }
    pub fn wake(&self) {
        self.ctx.request_repaint();
    }
    /// Bring the window back, e.g. from the tray menu or for an incoming request.
    pub fn show(&self) {
        // Windows sends no paint messages to a hidden window, so eframe would
        // never run another frame to apply the command below. Show it natively.
        #[cfg(windows)]
        win32::show(self.window.load(std::sync::atomic::Ordering::Relaxed));
        self.visible
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.wake();
    }
    pub fn event(&self, e: Event) {
        // The interface may not be drawn at all while the window is hidden
        // (Windows sends no paint messages), so tell the system instead.
        if !self.visible.load(std::sync::atomic::Ordering::Relaxed) {
            self.notify_hidden(&e);
        }
        self.events.lock().unwrap().push(e);
        self.wake();
    }
    fn notify_hidden(&self, e: &Event) {
        let (title, body) = match e {
            Event::Received { id } => {
                let Some(t) = self.transfer(*id) else { return };
                ("已收到文件", format!("「{}」发来 {}", t.peer, t.title()))
            }
            Event::Sent { id } => {
                let Some(t) = self.transfer(*id) else { return };
                ("已发送", format!("{} 已发送给「{}」", t.title(), t.peer))
            }
            Event::Failed { id } => {
                let Some(t) = self.transfer(*id) else { return };
                let what = if t.outgoing {
                    "发送失败"
                } else {
                    "接收失败"
                };
                (what, format!("{}：{}", t.title(), t.detail))
            }
            Event::Note(text) => ("邻传", text.clone()),
        };
        crate::notify::system(title, &body);
    }
    pub fn add(&self, t: Transfer) -> (u64, Arc<AtomicBool>) {
        let (id, cancel) = (t.id, t.cancel.clone());
        let mut list = self.transfers.lock().unwrap();
        while list.len() >= MAX_TRANSFERS {
            match list.iter().position(|x| x.stage.finished()) {
                Some(i) => {
                    list.remove(i);
                }
                None => break,
            }
        }
        list.push(t);
        drop(list);
        self.wake();
        (id, cancel)
    }
    pub fn update(&self, id: u64, f: impl FnOnce(&mut Transfer)) {
        if let Some(t) = self
            .transfers
            .lock()
            .unwrap()
            .iter_mut()
            .find(|t| t.id == id)
        {
            f(t);
            if t.stage.finished() && t.ended.is_none() {
                t.ended = Some(Instant::now());
            }
        }
        self.wake();
    }
    pub fn transfer(&self, id: u64) -> Option<Transfer> {
        self.transfers
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }
    /// Transfers running or waiting for an answer.
    pub fn active(&self) -> bool {
        self.transfers
            .lock()
            .unwrap()
            .iter()
            .any(|t| !t.stage.finished())
    }
    /// Change the settings and save them.
    pub fn change_settings(&self, f: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut s = self.settings.lock().unwrap();
        let mut next = s.clone();
        f(&mut next);
        if next == *s {
            return Ok(());
        }
        save_settings(&next)?;
        *s = next;
        drop(s);
        self.wake();
        Ok(())
    }
}

pub fn size(n: u64) -> String {
    if n >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", n as f64 / 1073741824.)
    } else if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1048576.)
    } else if n >= 1024 {
        format!("{:.0} KB", n as f64 / 1024.)
    } else {
        format!("{n} B")
    }
}

/// "12 秒", "3 分 20 秒", "1 小时 5 分".
pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{} 秒", s.max(1))
    } else if s < 3600 {
        format!("{} 分 {} 秒", s / 60, s % 60)
    } else {
        format!("{} 小时 {} 分", s / 3600, s % 3600 / 60)
    }
}

/// "刚刚", "5 分钟前", "2 小时前".
pub fn ago(t: Instant) -> String {
    let s = t.elapsed().as_secs();
    if s < 60 {
        "刚刚".into()
    } else if s < 3600 {
        format!("{} 分钟前", s / 60)
    } else if s < 86400 {
        format!("{} 小时前", s / 3600)
    } else {
        format!("{} 天前", s / 86400)
    }
}

#[cfg(windows)]
mod win32 {
    #[link(name = "user32")]
    extern "system" {
        fn ShowWindowAsync(hwnd: isize, cmd_show: i32) -> i32;
        fn SetForegroundWindow(hwnd: isize) -> i32;
    }
    const SW_SHOW: i32 = 5;
    pub fn show(hwnd: isize) {
        if hwnd != 0 {
            // SAFETY: plain user32 calls on our own top-level window; both
            // functions tolerate a stale handle by returning 0.
            unsafe {
                ShowWindowAsync(hwnd, SW_SHOW);
                SetForegroundWindow(hwnd);
            }
        }
    }
}
