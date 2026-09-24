//! Home page: nearby devices, what to send, and the send button.
use super::*;
use crate::network::{Payload, Target};
use egui::{text::LayoutJob, TextEdit, TextFormat};

/// Keyboard shortcut that sends text.
const SEND_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "⌘ Enter"
} else {
    "Ctrl+Enter"
};

/// One entry of the device grid.
struct Tile {
    key: String,
    name: String,
    sub: String,
    trusted: bool,
    platform: Platform,
    target: Target,
    manual: bool,
}

impl App {
    pub(super) fn home(&mut self, ui: &mut Ui) {
        self.devices(ui);
        ui.add_space(18.);
        self.content(ui);
    }

    fn tiles(&self) -> Vec<Tile> {
        let settings = self.shared.settings.lock().unwrap();
        let mut peers = self.shared.peers.lock().unwrap().clone();
        // Trusted devices first, then by name, so the grid does not jump around.
        peers.sort_by(|a, b| {
            (!settings.is_trusted(&a.id), &a.name).cmp(&(!settings.is_trusted(&b.id), &b.name))
        });
        let mut tiles: Vec<Tile> = peers
            .iter()
            .map(|p| {
                let trusted = settings.is_trusted(&p.id);
                Tile {
                    key: p.id.clone(),
                    name: p.name.clone(),
                    sub: if trusted {
                        "已信任".into()
                    } else {
                        p.platform.label().into()
                    },
                    trusted,
                    platform: p.platform,
                    target: Target {
                        address: p.address,
                        id: p.id.clone(),
                        name: p.name.clone(),
                    },
                    manual: false,
                }
            })
            .collect();
        for m in &self.manual {
            if peers.iter().any(|p| p.address.ip() == m.address.ip()) {
                continue;
            }
            let ip = m.address.ip().to_string();
            tiles.push(Tile {
                key: format!("ip:{}", m.address),
                name: ip.clone(),
                sub: "手动添加".into(),
                trusted: false,
                platform: Platform::Other,
                target: Target {
                    address: m.address,
                    id: String::new(),
                    name: ip,
                },
                manual: true,
            });
        }
        tiles
    }

    fn selected_tile(&self) -> Option<Tile> {
        let key = self.selected.as_ref()?;
        self.tiles().into_iter().find(|t| &t.key == key)
    }

    // ───────────────────────────── devices ─────────────────────────────

    fn devices(&mut self, ui: &mut Ui) {
        let tiles = self.tiles();
        // An address typed in has answered: select the discovered device.
        if let Some(address) = self
            .selected
            .as_deref()
            .and_then(|k| k.strip_prefix("ip:"))
            .and_then(|a| a.parse::<SocketAddr>().ok())
        {
            if let Some(t) = tiles
                .iter()
                .find(|t| !t.manual && t.target.address.ip() == address.ip())
            {
                self.selected = Some(t.key.clone());
            }
        }
        // Forget a selection whose device went away.
        if let Some(key) = &self.selected {
            if !tiles.iter().any(|t| &t.key == key) {
                self.selected = None;
            }
        }
        if self.selected.is_none() && tiles.len() == 1 {
            self.selected = Some(tiles[0].key.clone());
        }
        widgets::section_label(ui, "附近的设备");
        ui.add_space(-2.);
        if tiles.is_empty() {
            self.searching(ui);
            return;
        }
        let gap = 8.;
        let width = ui.available_width();
        let columns = ((width + gap) / (96. + gap)).floor().clamp(2., 6.) as usize;
        let tile_w = (width - gap * (columns - 1) as f32) / columns as f32;
        let count = tiles.len() + 1;
        let mut remove = None;
        for row in 0..count.div_ceil(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for i in row * columns..((row + 1) * columns).min(count) {
                    if let Some(tile) = tiles.get(i) {
                        let selected = self.selected.as_deref() == Some(tile.key.as_str());
                        let r = device_tile(ui, tile, selected, tile_w);
                        if r.clicked() {
                            self.selected = Some(tile.key.clone());
                        }
                        // Trusting happens only in the request dialog, after
                        // the handshake proved who the device is; the list
                        // comes from unauthenticated broadcasts.
                        if tile.manual || tile.trusted {
                            r.context_menu(|ui| {
                                ui.set_min_width(200.);
                                if tile.manual {
                                    if widgets::menu_item(ui, Icon::Trash, "移除此地址", true)
                                        .clicked()
                                    {
                                        remove = Some(tile.target.address);
                                        ui.close_menu();
                                    }
                                } else if widgets::menu_item(ui, Icon::Shield, "取消信任", false)
                                    .clicked()
                                {
                                    let id = tile.key.clone();
                                    self.change_settings(|s| s.trusted.retain(|t| t.id != id));
                                    ui.close_menu();
                                }
                            });
                        }
                    } else if add_tile(ui, tile_w).clicked() {
                        self.open_address_dialog();
                    }
                }
            });
        }
        if let Some(address) = remove {
            self.manual.retain(|m| m.address != address);
            self.shared
                .manual
                .lock()
                .unwrap()
                .retain(|ip| *ip != address.ip());
        }
    }

    fn open_address_dialog(&mut self) {
        self.address_dialog = AddressDialog {
            open: true,
            focus: true,
            ..Default::default()
        };
    }

    /// Empty state while no device has answered.
    fn searching(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(56., 56.), Sense::hover());
                let c = rect.center();
                // Slow pulse; repainting stops while the window is hidden.
                let t = ui.input(|i| i.time) as f32;
                for k in 0..2 {
                    let phase = (t / 2.2 + k as f32 * 0.5).fract();
                    ui.painter().circle_stroke(
                        c,
                        12. + phase * 16.,
                        Stroke::new(1.5, p.accent.gamma_multiply((1. - phase) * 0.55)),
                    );
                }
                ui.painter().circle_filled(c, 13., p.accent_soft);
                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(c, vec2(16., 16.)),
                    Icon::Search,
                    p.accent,
                );
                if self.shared.visible.load(Ordering::Relaxed) {
                    ui.ctx().request_repaint_after(Duration::from_millis(90));
                }
                ui.add_space(4.);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.;
                    ui.add_space(6.);
                    ui.label(
                        RichText::new("正在查找附近的电脑…")
                            .font(bold(13.5))
                            .color(p.text),
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new("在另一台电脑上打开邻传，并连接同一个 Wi-Fi 或局域网")
                                .font(body(12.))
                                .color(p.muted),
                        )
                        .wrap(),
                    );
                    ui.add_space(2.);
                    if Button::link("找不到？输入对方 IP 连接")
                        .small()
                        .show(ui)
                        .clicked()
                    {
                        self.open_address_dialog();
                    }
                });
            });
        });
    }

    // ───────────────────────────── content ─────────────────────────────

    fn content(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            widgets::section_label(ui, "要发送的内容");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let current = if self.mode == Mode::Files { 0 } else { 1 };
                if let Some(i) = widgets::segmented(
                    ui,
                    "mode",
                    current,
                    &[(Some(Icon::File), "文件"), (Some(Icon::Text), "文字")],
                    138.,
                ) {
                    self.mode = if i == 0 { Mode::Files } else { Mode::Text };
                }
            });
        });
        ui.add_space(-2.);
        match self.mode {
            Mode::Files if self.picked.is_empty() => self.drop_zone(ui),
            Mode::Files => self.picked_list(ui),
            Mode::Text => self.text_editor(ui),
        }
    }

    pub(super) fn pick_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("选择要发送的文件")
            .pick_files()
        {
            self.add_paths(files);
            self.mode = Mode::Files;
        }
    }

    fn pick_folders(&mut self) {
        if let Some(folders) = rfd::FileDialog::new()
            .set_title("选择要发送的文件夹")
            .pick_folders()
        {
            self.add_paths(folders);
            self.mode = Mode::Files;
        }
    }

    fn drop_zone(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let height = 176.;
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "选择要发送的文件")
        });
        let hot = response.hovered();
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            widgets::RADIUS,
            if hot {
                p.surface.lerp_to_gamma(p.accent_soft, 0.45)
            } else {
                p.surface
            },
        );
        widgets::dashed_rounded_rect(
            painter,
            rect.shrink(0.5),
            widgets::RADIUS as f32,
            Stroke::new(1.3, if hot { p.accent } else { p.border_strong }),
        );
        let c = pos2(rect.center().x, rect.top() + 46.);
        painter.circle_filled(c, 22., p.accent_soft);
        icons::paint(
            painter,
            Rect::from_center_size(c, vec2(22., 22.)),
            Icon::Upload,
            p.accent,
        );
        painter.text(
            c + vec2(0., 38.),
            Align2::CENTER_CENTER,
            "把文件或文件夹拖到这里",
            bold(14.),
            p.text,
        );
        // Two buttons under the hint.
        let row = Rect::from_center_size(pos2(rect.center().x, c.y + 78.), vec2(236., 30.));
        let mut files = false;
        let mut folders = false;
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(row)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.;
                files = Button::secondary("选择文件")
                    .icon(Icon::File)
                    .small()
                    .min_width(114.)
                    .show(ui)
                    .clicked();
                folders = Button::secondary("选择文件夹")
                    .icon(Icon::Folder)
                    .small()
                    .min_width(114.)
                    .show(ui)
                    .clicked();
            },
        );
        widgets::focus_ring(ui, &response, rect, widgets::RADIUS as f32);
        if files || (response.on_hover_cursor(CursorIcon::PointingHand).clicked() && !folders) {
            self.pick_files();
        } else if folders {
            self.pick_folders();
        }
    }

    fn picked_list(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let mut remove = None;
        let mut clear = false;
        let (mut add_files, mut add_folders) = (false, false);
        let known: Option<(u64, usize)> = self.picked.iter().try_fold((0, 0), |(b, n), f| {
            f.size.map(|(fb, fnum)| (b + fb, n + fnum))
        });
        widgets::card_frame(p)
            .inner_margin(Margin::same(0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.;
                // Summary and actions.
                Frame::new()
                    .inner_margin(Margin {
                        left: 14,
                        right: 8,
                        top: 8,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let summary = match known {
                                Some((bytes, files)) => {
                                    format!("{} 个文件 · {}", files, size(bytes))
                                }
                                None => format!("{} 项 · 正在统计…", self.picked.len()),
                            };
                            ui.label(RichText::new(summary).font(bold(13.)).color(p.text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 0.;
                                clear = Button::ghost("清空").small().show(ui).clicked();
                                add_folders = Button::ghost("文件夹")
                                    .icon(Icon::Plus)
                                    .small()
                                    .tooltip("添加文件夹")
                                    .show(ui)
                                    .clicked();
                                add_files = Button::ghost("文件")
                                    .icon(Icon::Plus)
                                    .small()
                                    .tooltip("添加文件")
                                    .show(ui)
                                    .clicked();
                            });
                        });
                    });
                widgets::separator(ui);
                egui::ScrollArea::vertical()
                    .id_salt("picked")
                    .max_height(262.)
                    .show(ui, |ui| {
                        for (i, f) in self.picked.iter().enumerate() {
                            if picked_row(ui, f, i > 0) {
                                remove = Some(i);
                            }
                        }
                    });
            });
        if clear {
            self.picked.clear();
        } else if let Some(i) = remove {
            self.picked.remove(i);
        }
        if add_files {
            self.pick_files();
        }
        if add_folders {
            self.pick_folders();
        }
    }

    fn text_editor(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let mut paste = false;
        widgets::card_frame(p)
            .inner_margin(Margin::same(0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                Frame::new()
                    .inner_margin(Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        let edit = TextEdit::multiline(&mut self.text)
                            .id_salt("text")
                            .frame(false)
                            .hint_text("输入要发送的文字或链接…")
                            .font(body(14.))
                            .desired_rows(6)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true);
                        egui::ScrollArea::vertical()
                            .id_salt("text-scroll")
                            .max_height(260.)
                            .show(ui, |ui| ui.add(edit));
                    });
                widgets::separator(ui);
                Frame::new()
                    .inner_margin(Margin {
                        left: 8,
                        right: 14,
                        top: 6,
                        bottom: 6,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.;
                            paste = Button::ghost("粘贴")
                                .icon(Icon::Paste)
                                .small()
                                .tooltip("用剪贴板中的文字替换")
                                .show(ui)
                                .clicked();
                            if !self.text.is_empty()
                                && Button::ghost("清空").small().show(ui).clicked()
                            {
                                self.text.clear();
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let n = self.text.chars().count();
                                let color = if self.text.len() > network::MAX_TEXT {
                                    p.danger
                                } else {
                                    p.muted
                                };
                                ui.label(
                                    RichText::new(format!("{n} 字"))
                                        .font(body(12.))
                                        .color(color),
                                );
                                ui.add_space(8.);
                                ui.label(
                                    RichText::new(format!("{SEND_SHORTCUT} 发送"))
                                        .font(body(12.))
                                        .color(p.faint),
                                );
                            });
                        });
                    });
            });
        if paste {
            match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                Ok(text) if !text.trim().is_empty() => self.text = text,
                _ => self.notify(Tone::Info, "剪贴板里没有文字。"),
            }
        }
    }

    // ───────────────────────────── bottom bar ─────────────────────────────

    pub(super) fn send_bar(&mut self, ui: &mut Ui) {
        self.activity_strip(ui);
        let target = self.selected_tile();
        let has_content = match self.mode {
            Mode::Files => !self.picked.is_empty(),
            Mode::Text => !self.text.trim().is_empty(),
        };
        let label = match (&target, has_content) {
            (None, _) => "先选择一台附近的设备".to_owned(),
            (Some(_), false) if self.mode == Mode::Files => "添加要发送的文件".to_owned(),
            (Some(_), false) => "输入要发送的文字".to_owned(),
            (Some(t), true) => format!("发送给 {}", ellipsize(&t.name, 16)),
        };
        let enabled = target.is_some() && has_content;
        let tip = format!("{SEND_SHORTCUT} 发送");
        let mut button = Button::primary(&label)
            .icon(Icon::ArrowUp)
            .enabled(enabled)
            .min_width(ui.available_width())
            .height(42.);
        if enabled && self.mode == Mode::Text {
            button = button.tooltip(&tip);
        }
        let clicked = button.show(ui).clicked();
        // ⌘/Ctrl+Enter sends text; consumed so the editor does not add a line.
        let shortcut = enabled
            && self.mode == Mode::Text
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter));
        if !(clicked || shortcut) {
            return;
        }
        let Some(tile) = target else {
            return;
        };
        let payload = match self.mode {
            Mode::Files => Payload::Files(self.picked.drain(..).map(|f| f.path).collect()),
            Mode::Text => {
                if self.text.len() > network::MAX_TEXT {
                    self.notify(Tone::Error, "文字过长，一次最多发送 256 KB。");
                    return;
                }
                Payload::Text(std::mem::take(&mut self.text))
            }
        };
        network::send(self.shared.clone(), tile.target, payload);
    }

    /// Progress of running transfers above the send button.
    fn activity_strip(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let running: Vec<Transfer> = self
            .shared
            .transfers
            .lock()
            .unwrap()
            .iter()
            .filter(|t| !t.stage.finished())
            .cloned()
            .collect();
        let Some(t) = running.first() else {
            return;
        };
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 50.), Sense::click());
        let response = response
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("查看传输记录");
        if response.clicked() {
            self.page = Page::Transfers;
        }
        let painter = ui.painter();
        painter.rect(
            rect,
            10,
            if response.hovered() {
                p.hover
            } else {
                p.surface
            },
            Stroke::new(1., p.border),
            egui::StrokeKind::Inside,
        );
        let icon = if t.outgoing {
            Icon::ArrowUp
        } else {
            Icon::ArrowDown
        };
        let c = pos2(rect.left() + 26., rect.center().y);
        widgets::icon_badge(ui, c, 15., icon, p.accent, p.accent_soft);
        let x = c.x + 26.;
        let right_w = 84.;
        let text_w = rect.right() - x - right_w - 12.;
        let title = transfers::headline(t);
        let g = widgets::galley(ui, &title, body(12.5), p.text, text_w);
        painter.galley(pos2(x, rect.top() + 9.), g, p.text);
        let bar = Rect::from_min_size(pos2(x, rect.top() + 32.), vec2(text_w, 4.));
        let fraction = t.fraction();
        let waiting = matches!(
            t.stage,
            Stage::Preparing | Stage::Connecting | Stage::Waiting
        );
        if waiting {
            // Indeterminate: a short segment sliding back and forth.
            let time = ui.input(|i| i.time) as f32;
            let pos = (time * 0.8).sin() * 0.5 + 0.5;
            widgets::paint_progress(painter, bar, 0., p.accent, p.subtle);
            let seg = Rect::from_min_size(
                pos2(bar.left() + pos * bar.width() * 0.7, bar.top()),
                vec2(bar.width() * 0.3, bar.height()),
            );
            painter.rect_filled(seg, 2, p.accent.gamma_multiply(0.7));
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        } else {
            widgets::paint_progress(painter, bar, fraction, p.accent, p.subtle);
        }
        let right = if waiting {
            transfers::waiting_text(t).to_owned()
        } else {
            format!("{:.0}%", fraction * 100.)
        };
        let g = widgets::galley(ui, &right, body(12.), p.text_2, right_w);
        painter.galley(
            pos2(rect.right() - 12. - g.size().x, rect.top() + 9.),
            g,
            p.text_2,
        );
        let more = if running.len() > 1 {
            format!("另有 {} 个", running.len() - 1)
        } else if t.speed() > 0. {
            format!("{}/s", size(t.speed() as u64))
        } else {
            String::new()
        };
        let g = widgets::galley(ui, &more, body(11.5), p.muted, right_w);
        painter.galley(
            pos2(rect.right() - 12. - g.size().x, rect.top() + 27.),
            g,
            p.muted,
        );
        ui.add_space(10.);
    }
}

/// Height of a device tile: avatar, up to two lines of name, and a label.
const TILE_HEIGHT: f32 = 126.;

fn device_tile(ui: &mut Ui, tile: &Tile, selected: bool, width: f32) -> egui::Response {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(width, TILE_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, selected, &tile.name)
    });
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, widgets::RADIUS, p.accent_soft);
        } else if hovered {
            painter.rect_filled(rect, widgets::RADIUS, p.hover);
        }
        let c = pos2(rect.center().x, rect.top() + 36.);
        let (fg, bg) = if selected {
            (p.on_accent, p.accent)
        } else {
            (p.text_2, if p.dark { p.subtle } else { p.surface })
        };
        if !selected {
            painter.circle_filled(c + vec2(0., 1.), 25.5, p.shadow);
            painter.circle_stroke(c, 25., Stroke::new(1., p.border));
        }
        widgets::avatar(painter, c, 25., tile.platform, fg, bg);
        let name = widgets::wrapped(
            ui,
            &tile.name,
            bold(12.5),
            if selected { p.accent } else { p.text },
            width - 12.,
            2,
        );
        let y = c.y + 33.;
        painter.galley(
            pos2(rect.center().x - name.rect.center().x, y),
            name.clone(),
            p.text,
        );
        let sub_color = if tile.trusted { p.success } else { p.muted };
        let sub = widgets::galley(ui, &tile.sub, body(11.), sub_color, width - 12.);
        painter.galley(
            pos2(rect.center().x - sub.size().x / 2., y + name.size().y + 1.),
            sub,
            sub_color,
        );
        widgets::focus_ring(ui, &response, rect, widgets::RADIUS as f32);
    }
    let mut tip = format!("{}\n{}", tile.name, tile.target.address.ip());
    if tile.manual {
        tip.push_str("\n右键可移除");
    } else if tile.trusted {
        tip.push_str("\n右键可取消信任");
    }
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tip)
}

/// "Connect by address" tile at the end of the grid.
fn add_tile(ui: &mut Ui, width: f32) -> egui::Response {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(width, TILE_HEIGHT), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "输入 IP"));
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, widgets::RADIUS, p.hover);
    }
    let c = pos2(rect.center().x, rect.top() + 36.);
    let pts: Vec<egui::Pos2> = (0..=64)
        .map(|i| {
            let a = i as f32 / 64. * std::f32::consts::TAU;
            c + 24.5 * vec2(a.cos(), a.sin())
        })
        .collect();
    painter.extend(egui::Shape::dashed_line(
        &pts,
        Stroke::new(1.3, if hovered { p.accent } else { p.border_strong }),
        4.,
        3.,
    ));
    icons::paint(
        painter,
        Rect::from_center_size(c, vec2(18., 18.)),
        Icon::Plus,
        if hovered { p.accent } else { p.muted },
    );
    let g = widgets::galley(ui, "输入 IP", body(12.5), p.text_2, width - 12.);
    painter.galley(
        pos2(rect.center().x - g.size().x / 2., c.y + 33.),
        g,
        p.text_2,
    );
    widgets::focus_ring(ui, &response, rect, widgets::RADIUS as f32);
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("通过 IP 地址连接不在列表中的电脑")
}

/// A row of the picked list; returns true when "remove" was clicked.
fn picked_row(ui: &mut Ui, f: &Picked, divider: bool) -> bool {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 50.), Sense::hover());
    if divider {
        ui.painter().hline(
            rect.left() + 56.0..=rect.right(),
            rect.top(),
            Stroke::new(1., p.separator),
        );
    }
    let tile = Rect::from_min_size(
        pos2(rect.left() + 14., rect.center().y - 15.),
        vec2(30., 30.),
    );
    widgets::file_tile(ui, tile, &f.name, f.dir);
    let detail = match (f.dir, f.size) {
        (true, Some((bytes, files))) => format!("{files} 个文件 · {}", size(bytes)),
        (true, None) => "文件夹 · 正在统计…".into(),
        (false, Some((bytes, _))) => size(bytes),
        (false, None) => String::new(),
    };
    let x = tile.right() + 12.;
    let close = Rect::from_center_size(pos2(rect.right() - 22., rect.center().y), vec2(26., 26.));
    let text_w = close.left() - 8. - x;
    let mut job = LayoutJob::default();
    job.append(&f.name, 0., TextFormat::simple(body(13.), p.text));
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
    let name = ui.fonts(|fonts| fonts.layout_job(job));
    let sub = widgets::galley(ui, &detail, body(11.5), p.muted, text_w);
    let top = rect.center().y - (name.size().y + sub.size().y) / 2.;
    ui.painter().galley(pos2(x, top), name.clone(), p.text);
    ui.painter()
        .galley(pos2(x, top + name.size().y), sub, p.muted);
    let hovered = response.hovered();
    let close_r = ui.interact(close, ui.id().with(("remove", &f.path)), Sense::click());
    let close_hot = close_r.hovered();
    if hovered || close_hot {
        ui.painter()
            .rect_filled(close, 7, if close_hot { p.pressed } else { p.hover });
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(close.center(), vec2(14., 14.)),
        Icon::Close,
        if close_hot { p.text } else { p.muted },
    );
    response.on_hover_text(f.path.display().to_string());
    close_r
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("移除")
        .clicked()
}
