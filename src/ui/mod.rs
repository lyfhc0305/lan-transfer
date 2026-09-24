//! Desktop interface. The home page picks a nearby device and what to send;
//! transfers and settings are sub-pages reached from the header.
#[cfg(feature = "demo")]
pub mod demo;
mod dialogs;
mod home;
mod icons;
mod settings;
mod theme;
mod transfers;
mod tray;
mod widgets;

pub use icons::icon;
pub use theme::configure;
pub use tray::make_tray;

use crate::{discovery, model::*, network};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, Context, CursorIcon, Frame, Id, Key, Layout, Margin,
    Order, Rect, RichText, Sense, Shadow, Stroke, Ui, UiBuilder,
};
use icons::Icon;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use theme::{body, bold, pal, palette};
use tray_icon::TrayIcon;
use widgets::Button;

/// Widest the content column grows when the window is enlarged.
const CONTENT_WIDTH: f32 = 560.;
/// Horizontal page padding.
const PAD: f32 = 18.;
const MAX_PICKED: usize = 500;
const TRAY_PLACE: &str = if cfg!(target_os = "macos") {
    "菜单栏"
} else {
    "系统托盘"
};
const FILE_MANAGER: &str = if cfg!(target_os = "macos") {
    "访达"
} else {
    "资源管理器"
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Transfers,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Files,
    Text,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tone {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug)]
enum ToastAction {
    Show(PathBuf),
    OpenFolder(PathBuf),
    Transfers,
}

struct Toast {
    tone: Tone,
    text: String,
    action: Option<(String, ToastAction)>,
    shown: Instant,
}

/// Something picked to send.
struct Picked {
    path: PathBuf,
    name: String,
    dir: bool,
    /// Size and file count; for folders filled in by a background count.
    size: Option<(u64, usize)>,
    counting: Option<mpsc::Receiver<(u64, usize)>>,
}

/// An address typed in by the user.
#[derive(Clone, Debug)]
struct Manual {
    address: SocketAddr,
}

#[derive(Default)]
struct AddressDialog {
    open: bool,
    input: String,
    error: Option<String>,
    focus: bool,
}

pub struct App {
    shared: Arc<Shared>,
    quit: Arc<AtomicBool>,
    tray: Option<TrayIcon>,
    page: Page,
    mode: Mode,
    /// Selected device: a peer ID, or "ip:<address>" for a typed-in address.
    selected: Option<String>,
    manual: Vec<Manual>,
    picked: Vec<Picked>,
    text: String,
    toast: Option<Toast>,
    address_dialog: AddressDialog,
    /// "Trust this device" in the request dialog, per request.
    trust_choice: (u64, bool),
    name_draft: String,
    exit_confirm: bool,
    /// Set once the user chose to end running tasks; the window closes when
    /// they have stopped (so temporary files are removed) or after 2 s.
    exit_deadline: Option<Instant>,
    exiting: bool,
    addresses: Vec<Ipv4Addr>,
    addresses_at: Option<Instant>,
    content_top: f32,
}

impl App {
    pub fn new(
        shared: Arc<Shared>,
        quit: Arc<AtomicBool>,
        tray: Option<TrayIcon>,
        warning: Option<String>,
    ) -> Self {
        let name_draft = shared.settings.lock().unwrap().name.clone();
        let mut app = Self {
            shared,
            quit,
            tray,
            page: Page::Home,
            mode: Mode::Files,
            selected: None,
            manual: vec![],
            picked: vec![],
            text: String::new(),
            toast: None,
            address_dialog: AddressDialog::default(),
            trust_choice: (0, false),
            name_draft,
            exit_confirm: false,
            exit_deadline: None,
            exiting: false,
            addresses: vec![],
            addresses_at: None,
            content_top: 64.,
        };
        if let Some(w) = warning {
            app.notify(Tone::Error, w);
        }
        app
    }

    fn notify(&mut self, tone: Tone, text: impl Into<String>) {
        self.toast = Some(Toast {
            tone,
            text: text.into(),
            action: None,
            shown: Instant::now(),
        });
    }

    fn notify_with(
        &mut self,
        tone: Tone,
        text: impl Into<String>,
        label: &str,
        action: ToastAction,
    ) {
        self.toast = Some(Toast {
            tone,
            text: text.into(),
            action: Some((label.to_owned(), action)),
            shown: Instant::now(),
        });
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut over = false;
        for path in paths {
            if self.picked.iter().any(|f| f.path == path) {
                continue;
            }
            if self.picked.len() >= MAX_PICKED {
                over = true;
                break;
            }
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            let dir = meta.is_dir();
            let (size, counting) = if dir {
                (
                    None,
                    Some(count_folder(path.clone(), self.shared.ctx.clone())),
                )
            } else {
                (Some((meta.len(), 1)), None)
            };
            self.picked.push(Picked {
                path,
                name,
                dir,
                size,
                counting,
            });
        }
        if over {
            self.notify(
                Tone::Info,
                format!("一次最多选择 {MAX_PICKED} 项，可以把文件放进文件夹后整体发送。"),
            );
        }
    }

    fn quit(&mut self, ctx: &Context) {
        if self.shared.active() {
            self.exit_confirm = true;
            self.shared.show();
        } else {
            self.close(ctx);
        }
    }

    fn close(&mut self, ctx: &Context) {
        self.exiting = true;
        discovery::goodbye(&self.shared);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn hide(&mut self, ctx: &Context) {
        self.shared.visible.store(false, Ordering::Relaxed);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn local_addresses(&mut self) -> Vec<Ipv4Addr> {
        // Interface enumeration is a system call; refresh every few seconds only.
        let stale = match self.addresses_at {
            Some(t) => t.elapsed() > Duration::from_secs(5),
            None => true,
        };
        if stale {
            self.addresses = network::local_addresses();
            self.addresses_at = Some(Instant::now());
        }
        self.addresses.clone()
    }

    fn change_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        if let Err(e) = self.shared.change_settings(f) {
            self.notify(Tone::Error, format!("无法保存设置：{e}"));
        }
    }

    /// Show background events (finished transfers, notes) as toasts.
    fn handle_events(&mut self) {
        let events: Vec<Event> = std::mem::take(&mut *self.shared.events.lock().unwrap());
        for e in events {
            match e {
                Event::Received { id } => {
                    let Some(t) = self.shared.transfer(id) else {
                        continue;
                    };
                    let text = match (t.items.as_slice(), t.files) {
                        ([one], 1) => format!("已收到「{}」发来的「{one}」", t.peer),
                        (_, n) => format!("已收到「{}」发来的 {n} 个文件", t.peer),
                    };
                    match t.saved.as_slice() {
                        [one] => self.notify_with(
                            Tone::Success,
                            text,
                            &format!("在{FILE_MANAGER}中显示"),
                            ToastAction::Show(one.clone()),
                        ),
                        _ => {
                            let folder = self.shared.settings.lock().unwrap().folder.clone();
                            self.notify_with(
                                Tone::Success,
                                text,
                                "打开文件夹",
                                ToastAction::OpenFolder(folder),
                            )
                        }
                    }
                }
                Event::Sent { id } => {
                    if let Some(t) = self.shared.transfer(id) {
                        self.notify(Tone::Success, format!("已发送给「{}」", t.peer));
                    }
                }
                Event::Failed { id } => {
                    if let Some(t) = self.shared.transfer(id) {
                        let what = if t.outgoing {
                            "发送失败"
                        } else {
                            "接收失败"
                        };
                        self.notify_with(
                            Tone::Error,
                            format!("{what}：{}", t.detail),
                            "查看",
                            ToastAction::Transfers,
                        );
                    }
                }
                Event::Note(text) => self.notify(Tone::Info, text),
            }
        }
    }

    // ───────────────────────────── header ─────────────────────────────

    fn header(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let height = 40.;
        ui.horizontal(|ui| {
            ui.set_height(height);
            if self.page == Page::Home {
                let (rect, _) = ui.allocate_exact_size(vec2(30., 30.), Sense::hover());
                icons::paint_logo(ui.painter(), rect);
                ui.add_space(2.);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.;
                    ui.add_space(1.);
                    ui.label(RichText::new("邻传").font(bold(15.5)).color(p.text));
                    self.status_line(ui);
                });
            } else {
                if widgets::icon_button(ui, Icon::Back, "返回").clicked() {
                    self.page = Page::Home;
                }
                let title = match self.page {
                    Page::Transfers => "传输记录",
                    _ => "设置",
                };
                ui.label(RichText::new(title).font(bold(15.5)).color(p.text));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                match self.page {
                    Page::Home => {
                        if widgets::icon_button(ui, Icon::Gear, "设置").clicked() {
                            self.page = Page::Settings;
                        }
                        let active = self
                            .shared
                            .transfers
                            .lock()
                            .unwrap()
                            .iter()
                            .filter(|t| !t.stage.finished())
                            .count();
                        if widgets::icon_button_badge(
                            ui,
                            Icon::Transfers,
                            "传输记录",
                            active,
                            false,
                        )
                        .clicked()
                        {
                            self.page = Page::Transfers;
                        }
                    }
                    Page::Transfers => self.transfers_actions(ui),
                    Page::Settings => {}
                }
            });
        });
    }

    /// "我的 MacBook · ● 可被发现" under the app name.
    fn status_line(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let settings = self.shared.settings.lock().unwrap().clone();
        let ready = self.shared.ready.load(Ordering::Relaxed);
        let receiver_note = self.shared.receiver_note.lock().unwrap().clone();
        let discovery_note = self.shared.discovery_note.lock().unwrap().clone();
        let (label, color, tip) = if !settings.receive {
            (
                "接收已关闭",
                p.muted,
                "其他电脑看不到本机，也无法发送文件。可在设置中打开".to_owned(),
            )
        } else if let Some(note) = receiver_note {
            ("无法接收", p.danger, note)
        } else if !ready {
            ("正在启动", p.muted, "正在启动接收服务…".to_owned())
        } else if let Some(note) = discovery_note {
            ("可接收", p.warning, note)
        } else if settings.trusted_only {
            (
                "仅接收信任设备",
                p.success,
                "只有已信任的设备可以发送文件".to_owned(),
            )
        } else {
            (
                "可被发现",
                p.success,
                "同一网络中的电脑可以看到本机并发送文件".to_owned(),
            )
        };
        let r = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.;
                widgets::truncated(
                    ui,
                    RichText::new(&settings.name).font(body(12.)).color(p.muted),
                );
                let (r, _) = ui.allocate_exact_size(vec2(6., 12.), Sense::hover());
                ui.painter().circle_filled(r.center(), 3., color);
                ui.label(RichText::new(label).font(body(12.)).color(p.muted));
            })
            .response;
        r.on_hover_text(tip);
    }

    // ───────────────────────────── overlays ─────────────────────────────

    fn toast(&mut self, ctx: &Context) {
        let Some(toast) = self.toast.as_ref() else {
            return;
        };
        let life = match toast.tone {
            Tone::Info | Tone::Success => Duration::from_secs(5),
            Tone::Error => Duration::from_secs(10),
        };
        let age = toast.shown.elapsed();
        if age >= life {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(life - age);
        let (tone, text, action) = (toast.tone, toast.text.clone(), toast.action.clone());
        let p = palette(&ctx.style().visuals);
        let (glyph, color) = match tone {
            Tone::Info => (Icon::InfoMark, p.accent),
            Tone::Success => (Icon::Check, p.success),
            Tone::Error => (Icon::Bang, p.danger),
        };
        let width = (ctx.screen_rect().width() - 2. * PAD).min(420.);
        let mut close = false;
        let mut run = None;
        egui::Area::new(Id::new("toast"))
            .order(Order::Foreground)
            .anchor(Align2::CENTER_TOP, vec2(0., self.content_top + 8.))
            .interactable(true)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1., p.border))
                    .corner_radius(12)
                    .inner_margin(Margin::symmetric(12, 10))
                    .shadow(Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: p.shadow_strong,
                    })
                    .show(ui, |ui| {
                        ui.set_width(width - 24.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.;
                            let (r, _) = ui.allocate_exact_size(vec2(20., 20.), Sense::hover());
                            ui.painter().circle_filled(r.center(), 10., color);
                            icons::paint(
                                ui.painter(),
                                Rect::from_center_size(r.center(), vec2(12., 12.)),
                                glyph,
                                Color32::WHITE,
                            );
                            let action_w = action.as_ref().map_or(0., |(label, _)| {
                                ui.fonts(|f| {
                                    f.layout_no_wrap(label.clone(), body(12.5), p.accent)
                                        .size()
                                        .x
                                }) + 20.
                            });
                            let text_w = ui.available_width() - 30. - action_w - 10.;
                            let g = ui.fonts(|f| f.layout(text.clone(), body(13.), p.text, text_w));
                            let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                            ui.painter().galley(r.min, g, p.text);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 2.;
                                if widgets::icon_button(ui, Icon::Close, "关闭").clicked() {
                                    close = true;
                                }
                                if let Some((label, act)) = &action {
                                    if Button::link(label).small().show(ui).clicked() {
                                        run = Some(act.clone());
                                        close = true;
                                    }
                                }
                            });
                        });
                    });
            });
        if let Some(act) = run {
            match act {
                ToastAction::Show(path) => self.reveal(&path),
                ToastAction::OpenFolder(path) => self.open_folder(&path),
                ToastAction::Transfers => self.page = Page::Transfers,
            }
        }
        if close {
            self.toast = None;
        }
    }

    fn drop_overlay(&self, ctx: &Context) {
        let hovered = ctx.input(|i| i.raw.hovered_files.len());
        if hovered == 0 {
            return;
        }
        let p = palette(&ctx.style().visuals);
        let painter = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("drop")));
        let screen = ctx.screen_rect();
        painter.rect_filled(screen, 0, p.bg.gamma_multiply(0.94));
        let inner = screen.shrink(14.);
        painter.rect_filled(inner, 16, p.accent_soft.gamma_multiply(0.85));
        widgets::dashed_rounded_rect(&painter, inner, 16., Stroke::new(2., p.accent));
        let c = inner.center() - vec2(0., 26.);
        painter.circle_filled(c, 30., p.accent);
        icons::paint(
            &painter,
            Rect::from_center_size(c, vec2(28., 28.)),
            Icon::Upload,
            p.on_accent,
        );
        painter.text(
            c + vec2(0., 52.),
            Align2::CENTER_CENTER,
            "松开以添加到发送列表",
            bold(16.),
            p.text,
        );
        painter.text(
            c + vec2(0., 76.),
            Align2::CENTER_CENTER,
            "文件和文件夹都可以",
            body(12.5),
            p.text_2,
        );
    }

    // ───────────────────────────── system actions ─────────────────────────────

    fn open_folder(&mut self, path: &Path) {
        if let Err(e) = open_folder(path) {
            self.notify(Tone::Error, format!("无法打开文件夹：{e}"));
        }
    }

    fn open_file(&mut self, path: &Path) {
        if let Err(e) = open::that_detached(path) {
            self.notify(Tone::Error, format!("无法打开：{e}"));
        }
    }

    fn reveal(&mut self, path: &Path) {
        if let Err(e) = reveal(path) {
            self.notify(Tone::Error, format!("无法在{FILE_MANAGER}中显示：{e}"));
        }
    }
}

/// Open a folder in Finder / Explorer, re-creating it if it was removed.
pub(crate) fn open_folder(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    open::that_detached(path)
}

/// Show a file selected in Finder / Explorer.
fn reveal(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        open::that_detached(path.parent().unwrap_or(path))
    }
}

/// Count a folder's files and bytes in the background.
fn count_folder(path: PathBuf, ctx: Context) -> mpsc::Receiver<(u64, usize)> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut total = (0u64, 0usize);
        let mut stack = vec![path];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in read.flatten() {
                let Ok(kind) = e.file_type() else {
                    continue;
                };
                if network::is_junk(&e.file_name().to_string_lossy()) {
                    continue;
                }
                if kind.is_dir() {
                    stack.push(e.path());
                } else if kind.is_file() {
                    total.0 += e.metadata().map(|m| m.len()).unwrap_or(0);
                    total.1 += 1;
                }
            }
        }
        let _ = tx.send(total);
        ctx.request_repaint();
    });
    rx
}

/// Lay out `add` in a centred column no wider than [`CONTENT_WIDTH`].
fn centered<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let full = ui.available_rect_before_wrap();
    let width = full.width().min(CONTENT_WIDTH);
    let rect = Rect::from_min_size(
        pos2(full.center().x - width / 2., full.top()),
        vec2(width, full.height()),
    );
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
        add,
    )
    .inner
}

fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let mut s: String = text.chars().take(max_chars - 1).collect();
        s.push('…');
        s
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl App {
    fn shortcuts(&mut self, ctx: &Context) {
        let (quit, close, open, escape) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::Q),
                i.modifiers.command && i.key_pressed(Key::W),
                i.modifiers.command && i.key_pressed(Key::O),
                i.key_pressed(Key::Escape),
            )
        });
        if quit {
            self.quit(ctx);
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if open && self.page == Page::Home {
            self.pick_files();
        }
        if escape && self.page != Page::Home && !self.address_dialog.open {
            self.page = Page::Home;
        }
    }

    fn ui(&mut self, ctx: &Context) {
        if self.quit.swap(false, Ordering::Relaxed) {
            self.quit(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.exiting {
            let hide = self.shared.settings.lock().unwrap().close_to_tray && self.tray.is_some();
            if hide {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide(ctx);
            } else if self.shared.active() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.exit_confirm = true;
            } else {
                self.exiting = true;
                discovery::goodbye(&self.shared);
            }
        }
        self.shortcuts(ctx);
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if !dropped.is_empty() {
            self.add_paths(dropped);
            self.mode = Mode::Files;
            self.page = Page::Home;
        }
        for f in &mut self.picked {
            if let Some(rx) = &f.counting {
                if let Ok(v) = rx.try_recv() {
                    f.size = Some(v);
                    f.counting = None;
                }
            }
        }
        self.handle_events();
        let p = palette(&ctx.style().visuals);
        egui::TopBottomPanel::top("header")
            .show_separator_line(false)
            .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                left: PAD as i8,
                right: (PAD - 6.) as i8,
                top: 12,
                bottom: 8,
            }))
            .show(ctx, |ui| centered(ui, |ui| self.header(ui)));
        if self.page == Page::Home {
            egui::TopBottomPanel::bottom("send-bar")
                .show_separator_line(false)
                .frame(Frame::new().fill(p.bg).inner_margin(Margin {
                    left: PAD as i8,
                    right: PAD as i8,
                    top: 12,
                    bottom: 14,
                }))
                .show(ctx, |ui| {
                    let top = ui.max_rect().top() - 12.;
                    ui.painter().hline(
                        ui.ctx().screen_rect().x_range(),
                        top,
                        Stroke::new(1., p.border),
                    );
                    centered(ui, |ui| self.send_bar(ui))
                });
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg))
            .show(ctx, |ui| {
                self.content_top = ui.max_rect().top();
                egui::ScrollArea::vertical()
                    .id_salt(("page", self.page as u8))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(4.);
                        let full = ui.available_rect_before_wrap();
                        let inner = full.shrink2(vec2(PAD, 0.));
                        ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
                            centered(ui, |ui| match self.page {
                                Page::Home => self.home(ui),
                                Page::Transfers => self.transfers_page(ui),
                                Page::Settings => self.settings_page(ui),
                            });
                        });
                        ui.add_space(18.);
                    });
            });
        if self.exit_confirm {
            self.exit_dialog(ctx);
        } else if !self.shared.requests.lock().unwrap().is_empty() {
            self.request_dialog(ctx);
        } else if !self.shared.messages.lock().unwrap().is_empty() {
            self.message_dialog(ctx);
        } else if self.address_dialog.open {
            self.address_dialog(ctx);
        }
        if let Some(deadline) = self.exit_deadline {
            if !self.shared.active() || Instant::now() >= deadline {
                self.close(ctx);
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        self.toast(ctx);
        self.drop_overlay(ctx);
        if self.shared.active() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &Context,
        app: &mut App,
        theme: egui::Theme,
        size: egui::Vec2,
    ) -> egui::FullOutput {
        ctx.set_theme(theme);
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ctx| app.ui(ctx),
        )
    }

    #[test]
    fn every_page_and_dialog_renders_in_light_and_dark() {
        let folder = tempfile::tempdir().unwrap();
        let ctx = Context::default();
        configure(&ctx);
        let (shared, mut app) = demo_app(&ctx, folder.path());
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            for size in [vec2(460., 660.), vec2(400., 540.), vec2(900., 800.)] {
                for page in [Page::Home, Page::Transfers, Page::Settings] {
                    for mode in [Mode::Files, Mode::Text] {
                        app.page = page;
                        app.mode = mode;
                        frame(&ctx, &mut app, theme, size);
                        let output = frame(&ctx, &mut app, theme, size);
                        assert!(!output.shapes.is_empty());
                    }
                }
            }
            // Dialogs, one after another as they are dismissed.
            app.exit_confirm = true;
            frame(&ctx, &mut app, theme, vec2(460., 660.));
            app.exit_confirm = false;
            frame(&ctx, &mut app, theme, vec2(460., 660.));
            app.address_dialog.open = true;
            frame(&ctx, &mut app, theme, vec2(460., 660.));
        }
        assert_eq!(shared.requests.lock().unwrap().len(), 1);
    }

    /// State used by the render test and the screenshots.
    fn demo_app(ctx: &Context, folder: &Path) -> (Arc<Shared>, App) {
        let settings = Settings {
            folder: folder.into(),
            name: "测试 Mac".into(),
            ..Settings::default()
        };
        let shared = Shared::new(settings, ctx.clone());
        *shared.peers.lock().unwrap() = vec![Peer {
            id: "a".repeat(64),
            name: "办公室电脑".into(),
            platform: Platform::Windows,
            address: format!("192.168.1.40:{PORT}").parse().unwrap(),
            seen: Instant::now(),
        }];
        let mut t = Transfer::new(true, "办公室电脑", "192.168.1.40");
        t.items = vec!["报告.pdf".into(), "相册".into()];
        t.files = 12;
        t.total = 1 << 30;
        t.done = 1 << 29;
        t.rate = 12e6;
        t.stage = Stage::Running;
        shared.add(t);
        let mut t = Transfer::new(false, "客厅电脑", "192.168.1.41");
        t.text = Some("https://example.com".into());
        t.stage = Stage::Done;
        shared.add(t);
        let (tx, _rx) = mpsc::sync_channel(1);
        std::mem::forget(_rx);
        shared.requests.lock().unwrap().push(Request {
            id: 7,
            peer: "办公室电脑".into(),
            peer_id: "a".repeat(64),
            platform: Platform::Windows,
            address: "192.168.1.40".into(),
            items: vec![ItemSummary {
                name: "合同.pdf".into(),
                dir: false,
                size: 2048,
                files: 1,
            }],
            files: 1,
            total: 2048,
            created: Instant::now(),
            decision: tx,
        });
        shared.messages.lock().unwrap().push(Message {
            id: 9,
            peer: "客厅电脑".into(),
            text: "会议链接 https://example.com".into(),
        });
        let mut app = App::new(
            shared.clone(),
            Arc::new(AtomicBool::new(false)),
            None,
            Some("提示".into()),
        );
        let file = folder.join("待发送.txt");
        std::fs::write(&file, b"hello").unwrap();
        app.add_paths(vec![file, folder.to_path_buf()]);
        app.text = "你好".into();
        (shared, app)
    }

    #[test]
    fn heading_font_covers_every_ui_character() {
        // The SemiBold font is a subset; make sure no UI string falls back
        // to the regular weight glyph by glyph.
        let ctx = Context::default();
        let mut defs = egui::FontDefinitions::empty();
        let fonts = theme::fonts();
        defs.font_data
            .insert("semibold".into(), fonts.font_data["noto-semibold"].clone());
        defs.families
            .insert(egui::FontFamily::Proportional, vec!["semibold".into()]);
        ctx.set_fonts(defs);
        let _ = ctx.run(Default::default(), |_| {});
        let sources = [
            include_str!("mod.rs"),
            include_str!("home.rs"),
            include_str!("transfers.rs"),
            include_str!("settings.rs"),
            include_str!("dialogs.rs"),
            include_str!("widgets.rs"),
            include_str!("../network.rs"),
            include_str!("../wire.rs"),
            include_str!("../discovery.rs"),
            include_str!("../model.rs"),
        ];
        let font = egui::FontId::proportional(14.);
        let mut missing = String::new();
        for line in sources.concat().lines() {
            // Characters inside string literals on non-comment lines.
            if line.trim_start().starts_with("//") {
                continue;
            }
            for (i, part) in line.split('"').enumerate() {
                if i % 2 == 1 {
                    for c in part.chars().filter(|c| !c.is_ascii()) {
                        if !ctx.fonts(|f| f.has_glyph(&font, c)) && !missing.contains(c) {
                            missing.push(c);
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "run scripts/make_fonts.py; SemiBold subset lacks: {missing}"
        );
    }

    #[test]
    fn short_text_helpers() {
        assert_eq!(ellipsize("abcdef", 4), "abc…");
        assert_eq!(ellipsize("abc", 4), "abc");
    }
}
