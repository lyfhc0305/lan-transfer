//! Settings page. Every change is saved at once.
use super::*;
use egui::TextEdit;

impl App {
    pub(super) fn settings_page(&mut self, ui: &mut Ui) {
        let p = pal(ui);
        let settings = self.shared.settings.lock().unwrap().clone();

        widgets::section_label(ui, "本机");
        ui.add_space(-2.);
        widgets::group(ui, |ui| {
            widgets::setting_row(
                ui,
                "设备名称",
                Some("附近的电脑看到的名称"),
                190.,
                |ui| {
                    let edit = ui.add(
                        TextEdit::singleline(&mut self.name_draft)
                            .id_salt("device-name")
                            .char_limit(32)
                            .horizontal_align(Align::RIGHT)
                            .margin(Margin::symmetric(8, 5))
                            .desired_width(186.),
                    );
                    let commit = edit.lost_focus()
                        || (edit.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                    if commit {
                        let name = self.name_draft.trim().to_owned();
                        if name.is_empty() {
                            self.name_draft = settings.name.clone();
                        } else if name != settings.name {
                            self.change_settings(|s| s.name = name.clone());
                            self.name_draft = name;
                            discovery::announce(&self.shared);
                        }
                    }
                },
            );
            widgets::separator(ui);
            let addresses = self.local_addresses();
            let shown = addresses
                .iter()
                .map(|ip| ip.to_string())
                .collect::<Vec<_>>()
                .join("、");
            widgets::setting_row(
                ui,
                "IP 地址",
                Some("对方找不到本机时，可输入此地址"),
                170.,
                |ui| {
                    if addresses.is_empty() {
                        ui.label(RichText::new("未连接网络").color(p.warning));
                    } else {
                        if widgets::icon_button(ui, Icon::Copy, "复制").clicked() {
                            ui.ctx().copy_text(addresses[0].to_string());
                            self.notify(Tone::Success, format!("已复制 {}", addresses[0]));
                        }
                        widgets::truncated(ui, RichText::new(shown).color(p.text_2));
                    }
                },
            );
        });

        ui.add_space(18.);
        widgets::section_label(ui, "接收");
        ui.add_space(-2.);
        widgets::group(ui, |ui| {
            let mut receive = settings.receive;
            if widgets::toggle_row(
                ui,
                &mut receive,
                "允许接收",
                Some("关闭后，其他电脑看不到本机，也无法发送"),
                true,
            ) {
                self.change_settings(|s| s.receive = receive);
                if receive {
                    discovery::announce(&self.shared);
                } else {
                    discovery::goodbye(&self.shared);
                }
            }
            widgets::separator(ui);
            let mut only = settings.trusted_only;
            if widgets::toggle_row(
                ui,
                &mut only,
                "只接收已信任设备",
                Some("其他设备的请求会被自动拒绝"),
                settings.receive,
            ) {
                self.change_settings(|s| s.trusted_only = only);
            }
            widgets::separator(ui);
            let folder = settings.folder.display().to_string();
            widgets::setting_row(ui, "保存位置", None, 250., |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                if Button::secondary("更改…").small().show(ui).clicked() {
                    if let Some(folder) = rfd::FileDialog::new()
                        .set_title("选择收到的文件保存在哪里")
                        .set_directory(&settings.folder)
                        .pick_folder()
                    {
                        self.change_settings(|s| s.folder = folder);
                    }
                }
                if widgets::icon_button(ui, Icon::Folder, &format!("在{FILE_MANAGER}中打开"))
                    .clicked()
                {
                    self.open_folder(&settings.folder);
                }
                widgets::truncated(ui, RichText::new(short_path(&folder)).color(p.text_2))
                    .on_hover_text(&folder);
            });
        });

        ui.add_space(18.);
        widgets::section_label(ui, "已信任的设备");
        ui.add_space(-2.);
        let mut untrust = None;
        widgets::group(ui, |ui| {
            if settings.trusted.is_empty() {
                ui.add_space(12.);
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            "收到文件时勾选「信任此设备」，以后这台设备发来的文件会直接保存，不再询问。",
                        )
                        .font(body(12.5))
                        .color(p.muted),
                    )
                    .wrap(),
                );
                ui.add_space(12.);
            }
            for (i, t) in settings.trusted.iter().enumerate() {
                if i > 0 {
                    widgets::separator(ui);
                }
                ui.horizontal(|ui| {
                    ui.set_height(52.);
                    let (r, _) = ui.allocate_exact_size(vec2(32., 32.), Sense::hover());
                    widgets::avatar(
                        ui.painter(),
                        r.center(),
                        16.,
                        t.platform,
                        p.text_2,
                        p.subtle,
                    );
                    ui.add_space(2.);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.;
                        ui.add_space(8.);
                        widgets::truncated(
                            ui,
                            RichText::new(&t.name).font(bold(13.)).color(p.text),
                        );
                        let mut detail = vec![t.platform.label().to_owned()];
                        if !t.address.is_empty() {
                            detail.push(t.address.clone());
                        }
                        detail.push(format!("指纹 {}", fingerprint(&t.id)));
                        widgets::truncated(
                            ui,
                            RichText::new(detail.join(" · "))
                                .font(body(11.5))
                                .color(p.muted),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if Button::new(widgets::Kind::Danger, "移除")
                            .small()
                            .show(ui)
                            .clicked()
                        {
                            untrust = Some(t.id.clone());
                        }
                    });
                });
            }
        });
        if let Some(id) = untrust {
            self.change_settings(|s| s.trusted.retain(|t| t.id != id));
        }

        if self.tray.is_some() {
            ui.add_space(18.);
            widgets::section_label(ui, "通用");
            ui.add_space(-2.);
            widgets::group(ui, |ui| {
                let mut stay = settings.close_to_tray;
                if widgets::toggle_row(
                    ui,
                    &mut stay,
                    &format!("关闭窗口后保留在{TRAY_PLACE}"),
                    Some("仍可接收文件；从图标菜单选择「退出邻传」才会完全退出"),
                    true,
                ) {
                    self.change_settings(|s| s.close_to_tray = stay);
                }
            });
        }

        ui.add_space(20.);
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 3.;
            ui.label(
                RichText::new(format!("邻传 {}", env!("CARGO_PKG_VERSION")))
                    .font(bold(12.))
                    .color(p.muted),
            );
            ui.label(
                RichText::new(format!(
                    "本机指纹 {} · 端口 {PORT}",
                    fingerprint(&self.shared.id())
                ))
                .font(body(11.5))
                .color(p.muted),
            )
            .on_hover_text("对方信任本机后，会在其设置中看到同样的指纹");
            ui.label(
                RichText::new("文件经加密直接在两台电脑之间传输，不经过云端")
                    .font(body(11.5))
                    .color(p.muted),
            );
        });
    }
}

/// "/Users/demo/Downloads/邻传接收" → "~/Downloads/邻传接收".
fn short_path(path: &str) -> String {
    if cfg!(windows) {
        return path.to_owned();
    }
    if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().display().to_string()) {
        if let Some(rest) = path.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    path.to_owned()
}
