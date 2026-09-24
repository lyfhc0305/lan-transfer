//! Modal dialogs: incoming request, received text, quit confirmation and
//! connecting by address.
use super::*;
use egui::{Modal, TextEdit};

fn dialog_frame(p: &theme::Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1., p.border))
        .corner_radius(16)
        .inner_margin(Margin::same(20))
        .shadow(Shadow {
            offset: [0, 16],
            blur: 48,
            spread: 0,
            color: p.shadow_strong,
        })
}

/// Width of a dialog that still fits a small window.
fn dialog_width(ctx: &Context, preferred: f32) -> f32 {
    preferred.min(ctx.screen_rect().width() - 2. * PAD - 40.)
}

/// Find the first http(s) link in a text. Links end at a space or at the
/// first non-ASCII character (Chinese punctuation right after a link).
fn first_link(text: &str) -> Option<&str> {
    let start = ["https://", "http://"]
        .iter()
        .filter_map(|p| text.find(p))
        .min()?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || !c.is_ascii() || "<>\"'()[]{}".contains(c))
        .unwrap_or(rest.len());
    let link = rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
    (link.len() > "https://".len()).then_some(link)
}

impl App {
    pub(super) fn request_dialog(&mut self, ctx: &Context) {
        let queue = self.shared.requests.lock().unwrap().clone();
        let Some(request) = queue.first() else {
            return;
        };
        if self.trust_choice.0 != request.id {
            self.trust_choice = (request.id, false);
        }
        let p = palette(&ctx.style().visuals);
        let remaining = CONSENT_TIMEOUT.saturating_sub(request.created.elapsed());
        let mut decision = None;
        let width = dialog_width(ctx, 360.);
        Modal::new(Id::new("incoming-request"))
            .backdrop_color(p.backdrop)
            .frame(dialog_frame(p))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.;
                    let (r, _) = ui.allocate_exact_size(vec2(52., 52.), Sense::hover());
                    widgets::avatar(
                        ui.painter(),
                        r.center(),
                        26.,
                        request.platform,
                        p.accent,
                        p.accent_soft,
                    );
                    ui.add_space(6.);
                    ui.label(
                        RichText::new(ellipsize(&request.peer, 24))
                            .font(bold(16.))
                            .color(p.text),
                    );
                    let what = match (&request.text, request.items.len(), request.files) {
                        (Some(_), _, _) => "一段文字".to_owned(),
                        (None, 1, _) if !request.items[0].dir => {
                            format!("1 个文件（{}）", size(request.total))
                        }
                        (None, _, files) => format!("{files} 个文件（{}）", size(request.total)),
                    };
                    ui.label(
                        RichText::new(format!("想发给你 {what}"))
                            .font(body(13.))
                            .color(p.text_2),
                    );
                });
                ui.add_space(10.);
                // What is coming: the text, or every item (scrolling when long).
                Frame::new()
                    .fill(p.bg)
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 0.;
                        // Shorter in a small window so the buttons stay visible.
                        let list_height = (ctx.screen_rect().height() - 400.).clamp(90., 200.);
                        egui::ScrollArea::vertical()
                            .id_salt(("request", request.id))
                            .max_height(list_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                if let Some(text) = &request.text {
                                    ui.add_space(8.);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(text).font(body(13.5)).color(p.text),
                                        )
                                        .wrap()
                                        .selectable(false),
                                    );
                                    ui.add_space(8.);
                                }
                                for item in &request.items {
                                    request_item(ui, item);
                                }
                            });
                    });
                if let Some(free) = request.free.filter(|free| *free < request.total) {
                    ui.add_space(8.);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "接收文件夹所在磁盘只剩 {}，不够保存这批文件。",
                                size(free)
                            ))
                            .font(body(12.))
                            .color(p.danger),
                        )
                        .wrap(),
                    );
                }
                ui.add_space(12.);
                widgets::checkbox(ui, &mut self.trust_choice.1, "信任此设备，以后自动接收");
                ui.add_space(4.);
                ui.label(
                    RichText::new(format!(
                        "{} · {} · 指纹 {}",
                        request.platform.label(),
                        request.address,
                        fingerprint(&request.peer_id)
                    ))
                    .font(body(11.5))
                    .color(p.muted),
                )
                .on_hover_text("可与对方「设置」底部显示的本机指纹核对");
                ui.add_space(14.);
                ui.columns(2, |cols| {
                    let w = cols[0].available_width();
                    if Button::secondary("拒绝")
                        .min_width(w)
                        .height(38.)
                        .show(&mut cols[0])
                        .clicked()
                    {
                        decision = Some(false);
                    }
                    if Button::primary("接收")
                        .min_width(w)
                        .height(38.)
                        .show(&mut cols[1])
                        .clicked()
                    {
                        decision = Some(true);
                    }
                });
                ui.add_space(6.);
                let mut note = format!("{} 秒后自动拒绝", remaining.as_secs());
                if queue.len() > 1 {
                    note.push_str(&format!(" · 还有 {} 个请求", queue.len() - 1));
                }
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(note).font(body(11.5)).color(p.muted));
                });
            });
        if let Some(accept) = decision {
            let _ = request.decision.try_send(Answer {
                accept,
                trust: accept && self.trust_choice.1,
            });
            let id = request.id;
            self.shared.requests.lock().unwrap().retain(|x| x.id != id);
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    pub(super) fn message_dialog(&mut self, ctx: &Context) {
        let Some(message) = self.shared.messages.lock().unwrap().first().cloned() else {
            return;
        };
        let p = palette(&ctx.style().visuals);
        let width = dialog_width(ctx, 380.);
        let mut done = false;
        let link = first_link(&message.text).map(str::to_owned);
        Modal::new(Id::new("incoming-text"))
            .backdrop_color(p.backdrop)
            .frame(dialog_frame(p))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(34., 34.), Sense::hover());
                    widgets::icon_badge(
                        ui,
                        r.center(),
                        17.,
                        Icon::Message,
                        p.accent,
                        p.accent_soft,
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        ui.label(RichText::new("收到文字").font(bold(15.)).color(p.text));
                        widgets::truncated(
                            ui,
                            RichText::new(format!("来自 {}", message.peer))
                                .font(body(12.))
                                .color(p.muted),
                        );
                    });
                });
                ui.add_space(12.);
                Frame::new()
                    .fill(p.bg)
                    .corner_radius(10)
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt(("message", message.id))
                            .max_height(220.)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&message.text).font(body(14.)).color(p.text),
                                    )
                                    .wrap()
                                    .selectable(true),
                                );
                            });
                    });
                ui.add_space(16.);
                ui.horizontal(|ui| {
                    if Button::ghost("关闭").show(ui).clicked() {
                        done = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Button::primary("复制")
                            .icon(Icon::Copy)
                            .min_width(88.)
                            .show(ui)
                            .clicked()
                        {
                            ui.ctx().copy_text(message.text.clone());
                            self.notify(Tone::Success, "已复制到剪贴板。");
                        }
                        if let Some(link) = &link {
                            if Button::secondary("打开链接")
                                .icon(Icon::External)
                                .show(ui)
                                .on_hover_text(link)
                                .clicked()
                            {
                                if let Err(e) = open::that_detached(link) {
                                    self.notify(Tone::Error, format!("无法打开链接：{e}"));
                                }
                                done = true;
                            }
                        }
                    });
                });
            });
        if done || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.shared
                .messages
                .lock()
                .unwrap()
                .retain(|m| m.id != message.id);
        }
    }

    pub(super) fn exit_dialog(&mut self, ctx: &Context) {
        let p = palette(&ctx.style().visuals);
        let width = dialog_width(ctx, 320.);
        let response = Modal::new(Id::new("exit-confirm"))
            .backdrop_color(p.backdrop)
            .frame(dialog_frame(p))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(48., 48.), Sense::hover());
                    widgets::icon_badge(
                        ui,
                        r.center(),
                        24.,
                        Icon::Warning,
                        p.warning,
                        p.warning_soft,
                    );
                    ui.add_space(4.);
                    ui.label(RichText::new("退出邻传？").font(bold(16.)).color(p.text));
                    ui.label(
                        RichText::new("还有正在进行的传输，退出会中断它们。")
                            .font(body(13.))
                            .color(p.text_2),
                    );
                });
                ui.add_space(14.);
                if self.exit_deadline.is_some() {
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(18.).color(p.accent));
                        ui.label(RichText::new("正在结束传输…").color(p.text_2));
                    });
                    return;
                }
                ui.columns(2, |cols| {
                    let w = cols[0].available_width();
                    if Button::secondary("继续传输")
                        .min_width(w)
                        .height(38.)
                        .show(&mut cols[0])
                        .clicked()
                    {
                        self.exit_confirm = false;
                    }
                    if Button::new(widgets::Kind::DangerSolid, "中断并退出")
                        .min_width(w)
                        .height(38.)
                        .show(&mut cols[1])
                        .clicked()
                    {
                        for t in self.shared.transfers.lock().unwrap().iter() {
                            t.cancel.store(true, Ordering::Relaxed);
                        }
                        for r in self.shared.requests.lock().unwrap().iter() {
                            let _ = r.decision.try_send(Answer {
                                accept: false,
                                trust: false,
                            });
                        }
                        self.exit_deadline = Some(Instant::now() + Duration::from_secs(2));
                    }
                });
            });
        if response.should_close() && self.exit_deadline.is_none() {
            self.exit_confirm = false;
        }
    }

    pub(super) fn address_dialog(&mut self, ctx: &Context) {
        let p = palette(&ctx.style().visuals);
        let width = dialog_width(ctx, 340.);
        let mut submit = false;
        let mut cancel = false;
        let response = Modal::new(Id::new("address"))
            .backdrop_color(p.backdrop)
            .frame(dialog_frame(p))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.label(RichText::new("通过 IP 地址连接").font(bold(16.)).color(p.text));
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "对方的地址显示在它的「设置 › IP 地址」中。两台电脑需要在同一局域网，且对方已打开邻传。",
                        )
                        .font(body(12.5))
                        .color(p.text_2),
                    )
                    .wrap(),
                );
                ui.add_space(8.);
                let edit = ui.add(
                    TextEdit::singleline(&mut self.address_dialog.input)
                        .id_salt("address-input")
                        .hint_text("例如 192.168.1.20")
                        .char_limit(64)
                        .margin(Margin::symmetric(10, 8))
                        .desired_width(f32::INFINITY),
                );
                if self.address_dialog.focus {
                    edit.request_focus();
                    self.address_dialog.focus = false;
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
                if let Some(error) = &self.address_dialog.error {
                    ui.label(RichText::new(error).font(body(12.)).color(p.danger));
                }
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Button::primary("连接")
                            .min_width(80.)
                            .enabled(!self.address_dialog.input.trim().is_empty())
                            .show(ui)
                            .clicked()
                        {
                            submit = true;
                        }
                        if Button::ghost("取消").show(ui).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if submit {
            match network::parse_address(&self.address_dialog.input) {
                Ok(address)
                    if network::local_addresses()
                        .iter()
                        .any(|ip| address.ip() == *ip) =>
                {
                    self.address_dialog.error =
                        Some("这是本机的地址，请输入另一台电脑的地址。".into());
                }
                Ok(address) => {
                    discovery::probe(&self.shared, address.ip());
                    if !self.manual.iter().any(|m| m.address == address) {
                        self.manual.push(Manual { address });
                    }
                    self.selected = Some(format!("ip:{address}"));
                    self.address_dialog.open = false;
                }
                Err(e) => self.address_dialog.error = Some(e),
            }
        }
        if cancel || (response.should_close() && !submit) {
            self.address_dialog.open = false;
        }
    }
}

/// One row of the request dialog's list.
fn request_item(ui: &mut Ui, item: &ItemSummary) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.set_height(40.);
        let (r, _) = ui.allocate_exact_size(vec2(28., 28.), Sense::hover());
        widgets::file_tile(ui, r, &item.name, item.dir);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let detail = if item.dir {
                format!("{} 个文件 · {}", item.files, size(item.size))
            } else {
                size(item.size)
            };
            ui.label(RichText::new(detail).font(body(12.)).color(p.muted));
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                widgets::truncated(ui, RichText::new(&item.name).font(body(13.)).color(p.text))
                    .on_hover_text(&item.name);
            });
        });
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_are_found_in_text() {
        assert_eq!(
            super::first_link("会议：https://example.com/a?b=1，请准时"),
            Some("https://example.com/a?b=1")
        );
        assert_eq!(super::first_link("没有链接"), None);
        assert_eq!(
            super::first_link("链接：https://meeting.example.com/j/8842\n密码"),
            Some("https://meeting.example.com/j/8842")
        );
        assert_eq!(super::first_link("see http://a.b/c."), Some("http://a.b/c"));
    }
}
