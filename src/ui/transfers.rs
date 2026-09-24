//! Transfers page: running and finished batches of this session.
use super::*;
use crate::network::{Payload, Target};

/// One line for the progress strip on the home page.
pub(super) fn headline(t: &Transfer) -> String {
    let what = t.title();
    if t.outgoing {
        format!("发送 {what} 给 {}", t.peer)
    } else {
        format!("接收来自 {} 的 {what}", t.peer)
    }
}

/// Short state while nothing is being transferred yet.
pub(super) fn waiting_text(t: &Transfer) -> &'static str {
    match (t.stage, t.outgoing) {
        (Stage::Preparing, _) => "正在准备",
        (Stage::Connecting, _) => "正在连接",
        (Stage::Waiting, true) => "等待对方接受",
        (Stage::Waiting, false) => "等待你确认",
        _ => "",
    }
}

enum Action {
    Cancel(u64),
    Open(PathBuf),
    Reveal(PathBuf),
    OpenFolder,
    Copy(String),
    Retry(u64),
}

impl App {
    pub(super) fn transfers_actions(&mut self, ui: &mut Ui) {
        let finished = self
            .shared
            .transfers
            .lock()
            .unwrap()
            .iter()
            .any(|t| t.stage.finished());
        if Button::ghost("清除已完成")
            .small()
            .enabled(finished)
            .show(ui)
            .clicked()
        {
            self.shared
                .transfers
                .lock()
                .unwrap()
                .retain(|t| !t.stage.finished());
        }
    }

    pub(super) fn transfers_page(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let transfers = self.shared.transfers.lock().unwrap().clone();
        // Keep "3 分钟前" current.
        ui.ctx().request_repaint_after(Duration::from_secs(30));
        if transfers.is_empty() {
            ui.add_space(60.);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(56., 56.), Sense::hover());
                ui.painter().circle_filled(rect.center(), 28., p.subtle);
                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(rect.center(), vec2(26., 26.)),
                    Icon::Transfers,
                    p.muted,
                );
                ui.add_space(4.);
                ui.label(
                    RichText::new("还没有传输记录")
                        .font(bold(14.))
                        .color(p.text),
                );
                ui.label(
                    RichText::new("本次运行期间发送和接收的内容会显示在这里")
                        .font(body(12.5))
                        .color(p.muted),
                );
            });
            return;
        }
        // Running and waiting first (oldest first, like a queue), then the
        // finished ones, newest first.
        let running: Vec<&Transfer> = transfers.iter().filter(|t| !t.stage.finished()).collect();
        let finished: Vec<&Transfer> = transfers
            .iter()
            .rev()
            .filter(|t| t.stage.finished())
            .collect();
        let mut action = None;
        for (label, list) in [("进行中", &running), ("已完成", &finished)] {
            if list.is_empty() {
                continue;
            }
            widgets::section_label(ui, label);
            ui.add_space(-2.);
            widgets::group(ui, |ui| {
                for (i, t) in list.iter().enumerate() {
                    if i > 0 {
                        widgets::separator(ui);
                    }
                    if let Some(a) = transfer_row(ui, t) {
                        action = Some(a);
                    }
                }
            });
            ui.add_space(14.);
        }
        match action {
            Some(Action::Cancel(id)) => {
                if let Some(t) = self.shared.transfer(id) {
                    t.cancel.store(true, Ordering::Relaxed);
                }
                // An incoming request still on screen is declined as well.
                if let Some(r) = self
                    .shared
                    .requests
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|r| r.id == id)
                {
                    let _ = r.decision.try_send(Answer {
                        accept: false,
                        trust: false,
                    });
                }
            }
            Some(Action::Open(path)) => self.open_file(&path),
            Some(Action::Reveal(path)) => self.reveal(&path),
            Some(Action::OpenFolder) => {
                let folder = self.shared.settings.lock().unwrap().folder.clone();
                self.open_folder(&folder);
            }
            Some(Action::Copy(text)) => {
                ui.ctx().copy_text(text);
                self.notify(Tone::Success, "已复制到剪贴板。");
            }
            Some(Action::Retry(id)) => self.retry(id),
            None => {}
        }
    }

    fn retry(&mut self, id: u64) {
        let Some(t) = self.shared.transfer(id) else {
            return;
        };
        let Some((address, device)) = t.target.clone() else {
            return;
        };
        let payload = match &t.text {
            Some(text) => Payload::Text(text.clone()),
            None => Payload::Files(t.sources.clone()),
        };
        let target = Target {
            address,
            id: device,
            name: t.peer.clone(),
        };
        network::send(self.shared.clone(), target, payload);
    }
}

fn transfer_row(ui: &mut Ui, t: &Transfer) -> Option<Action> {
    let p = pal(ui);
    let mut action = None;
    let running = !t.stage.finished();
    let (fg, bg) = match t.stage {
        _ if running => (p.accent, p.accent_soft),
        Stage::Done => (p.success, p.success_soft),
        Stage::Failed => (p.danger, p.danger_soft),
        _ => (p.muted, p.subtle),
    };
    let icon = match (&t.text, t.outgoing) {
        (Some(_), _) => Icon::Message,
        (None, true) => Icon::ArrowUp,
        (None, false) => Icon::ArrowDown,
    };
    ui.add_space(10.);
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(34., 34.), Sense::hover());
        widgets::icon_badge(ui, r.center(), 17., icon, fg, bg);
        ui.add_space(2.);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.;
            // Title with actions on the right.
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    for a in actions(t).into_iter().rev() {
                        let (label, kind, act) = a;
                        if Button::new(kind, label).small().show(ui).clicked() {
                            action = Some(act);
                        }
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        widgets::truncated(
                            ui,
                            RichText::new(t.title()).font(bold(13.5)).color(p.text),
                        )
                        .on_hover_text(t.items.join("\n"));
                    });
                });
            });
            let mut parts = vec![if t.outgoing {
                format!("发送到 {}", t.peer)
            } else {
                format!("来自 {}", t.peer)
            }];
            if t.text.is_some() {
                parts.push("文字".into());
            } else {
                if t.files > 1 {
                    parts.push(format!("{} 个文件", t.files));
                }
                if t.total > 0 || t.stage == Stage::Done {
                    parts.push(size(t.total));
                }
            }
            parts.push(ago(t.started));
            widgets::truncated(
                ui,
                RichText::new(parts.join(" · "))
                    .font(body(12.))
                    .color(p.muted),
            )
            .on_hover_text(format!("{} · {}", t.peer, t.address));
            if running {
                ui.add_space(3.);
                let waiting = t.stage != Stage::Running;
                if waiting {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(12.).color(p.accent));
                        ui.label(
                            RichText::new(waiting_text(t))
                                .font(body(12.))
                                .color(p.text_2),
                        );
                    });
                } else {
                    widgets::progress(ui, t.fraction(), p.accent);
                    ui.horizontal(|ui| {
                        let mut left = format!("{:.0}%", t.fraction() * 100.);
                        if t.rate > 0. {
                            left.push_str(&format!(" · {}/s", size(t.rate as u64)));
                        }
                        if let Some(eta) = t.eta() {
                            left.push_str(&format!(" · 剩余 {}", duration(eta)));
                        }
                        ui.label(RichText::new(left).font(body(12.)).color(p.text_2));
                        if t.files > 1 && !t.current.is_empty() {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                widgets::truncated(
                                    ui,
                                    RichText::new(format!(
                                        "{}（{}/{}）",
                                        t.current,
                                        (t.files_done + 1).min(t.files),
                                        t.files
                                    ))
                                    .font(body(12.))
                                    .color(p.muted),
                                );
                            });
                        }
                    });
                }
            } else {
                let (glyph, text) = match t.stage {
                    Stage::Done if t.outgoing => (Icon::Check, "已发送".to_owned()),
                    Stage::Done => (Icon::Check, "已接收".to_owned()),
                    Stage::Failed => (Icon::Warning, t.detail.clone()),
                    _ => (Icon::Close, t.detail.clone()),
                };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.;
                    let (r, _) = ui.allocate_exact_size(vec2(13., 17.), Sense::hover());
                    icons::paint(
                        ui.painter(),
                        Rect::from_center_size(r.center(), vec2(13., 13.)),
                        glyph,
                        fg,
                    );
                    ui.add(
                        egui::Label::new(RichText::new(text).font(body(12.)).color(fg))
                            .wrap()
                            .selectable(false),
                    );
                });
                if t.stage == Stage::Done && !t.detail.is_empty() {
                    ui.label(RichText::new(&t.detail).font(body(11.5)).color(p.muted));
                }
            }
        });
    });
    ui.add_space(10.);
    action
}

/// Buttons for a transfer row, left to right.
fn actions(t: &Transfer) -> Vec<(&'static str, widgets::Kind, Action)> {
    use widgets::Kind;
    let mut list = vec![];
    if !t.stage.finished() {
        list.push(("取消", Kind::Danger, Action::Cancel(t.id)));
        return list;
    }
    if let Some(text) = &t.text {
        list.push(("复制", Kind::Ghost, Action::Copy(text.clone())));
    }
    if !t.outgoing && t.text.is_none() {
        match t.saved.as_slice() {
            [] => {}
            [one] => {
                list.push(("打开", Kind::Ghost, Action::Open(one.clone())));
                list.push(("显示", Kind::Ghost, Action::Reveal(one.clone())));
            }
            [first, ..] => {
                list.push(("显示", Kind::Ghost, Action::Reveal(first.clone())));
                list.push(("打开文件夹", Kind::Ghost, Action::OpenFolder));
            }
        }
    }
    if t.outgoing
        && matches!(t.stage, Stage::Failed | Stage::Declined | Stage::Cancelled)
        && t.target.is_some()
    {
        list.push(("重试", Kind::Ghost, Action::Retry(t.id)));
    }
    list
}
