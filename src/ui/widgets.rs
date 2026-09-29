//! Custom widgets shared by the pages: cards and grouped rows, buttons,
//! switches, the segmented control, avatars, progress bars and file tiles.
use super::icons::{self, Icon};
use super::theme::{body, bold, pal, Palette};
use crate::model::Platform;
use eframe::egui::{
    self, pos2,
    text::{CCursorRange, LayoutJob},
    vec2, Align2, Color32, CornerRadius, CursorIcon, Event, FontId, Frame, Galley, Id, Key,
    KeyboardShortcut, Margin, Modifiers, PointerButton, Pos2, Rect, Response, RichText, Sense,
    Shadow, Shape, Stroke, StrokeKind, TextFormat, TextWrapMode, Ui, WidgetInfo, WidgetType,
};
use std::sync::Arc;

pub const RADIUS: u8 = 12;

pub fn card_frame(p: &Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1., p.border))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(14))
        .shadow(Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: p.shadow,
        })
}

/// A full-width card.
pub fn card<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    card_frame(p)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// A card for rows separated by hairlines, like a settings group.
pub fn group<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    card_frame(p)
        .inner_margin(Margin::symmetric(14, 2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.;
            add_contents(ui)
        })
        .inner
}

/// Hairline between rows of a [`group`].
pub fn separator(ui: &mut Ui) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1., p.separator),
    );
}

/// Small heading above a card.
pub fn section_label(ui: &mut Ui, text: &str) -> Response {
    let p = pal(ui);
    ui.add(egui::Label::new(RichText::new(text).font(bold(12.5)).color(p.muted)).selectable(false))
}

/// Keyboard-focus outline for custom widgets.
pub fn focus_ring(ui: &Ui, response: &Response, rect: Rect, radius: f32) {
    if response.has_focus() {
        let p = pal(ui);
        ui.painter().rect_stroke(
            rect.expand(2.),
            radius + 2.,
            Stroke::new(2., p.accent.gamma_multiply(0.7)),
            StrokeKind::Outside,
        );
    }
}

/// Single-line text, truncated with "…" when wider than `max_width`.
pub fn galley(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(1.));
    ui.fonts(|f| f.layout_job(job))
}

/// Text wrapped to `width`, at most `rows` lines (the last one truncated).
pub fn wrapped(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    rows: usize,
) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(1.),
        max_rows: rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job.halign = egui::Align::Center;
    ui.fonts(|f| f.layout_job(job))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    /// Accent-coloured text without a background.
    Link,
    Danger,
    DangerSolid,
}

pub struct Button<'a> {
    text: &'a str,
    icon: Option<Icon>,
    kind: Kind,
    enabled: bool,
    small: bool,
    min_width: f32,
    height: Option<f32>,
    tooltip: Option<&'a str>,
}

impl<'a> Button<'a> {
    pub fn new(kind: Kind, text: &'a str) -> Self {
        Self {
            text,
            icon: None,
            kind,
            enabled: true,
            small: false,
            min_width: 0.,
            height: None,
            tooltip: None,
        }
    }
    pub fn primary(text: &'a str) -> Self {
        Self::new(Kind::Primary, text)
    }
    pub fn secondary(text: &'a str) -> Self {
        Self::new(Kind::Secondary, text)
    }
    pub fn ghost(text: &'a str) -> Self {
        Self::new(Kind::Ghost, text)
    }
    pub fn link(text: &'a str) -> Self {
        Self::new(Kind::Link, text)
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }
    pub fn tooltip(mut self, text: &'a str) -> Self {
        self.tooltip = Some(text);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let p = pal(ui);
        let enabled = self.enabled && ui.is_enabled();
        let solid = matches!(self.kind, Kind::Primary | Kind::DangerSolid);
        let size = if self.small { 12.5 } else { 13.5 };
        let font = if solid { bold(size) } else { body(size) };
        let (pad, icon_size, gap) = if self.small {
            (10., 14., 5.)
        } else {
            (14., 16., 6.)
        };
        let pad = if self.kind == Kind::Link { 4. } else { pad };
        let height = self.height.unwrap_or(if self.small { 28. } else { 34. });
        let galley = ui
            .fonts(|f| f.layout_no_wrap(self.text.to_owned(), font.clone(), Color32::PLACEHOLDER));
        let icon_w = match (self.icon, self.text.is_empty()) {
            (Some(_), true) => icon_size,
            (Some(_), false) => icon_size + gap,
            (None, _) => 0.,
        };
        let content_w = icon_w + galley.size().x;
        let width = (content_w + 2. * pad).max(self.min_width).max(height);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, self.text));
        if ui.is_rect_visible(rect) {
            let pressed = response.is_pointer_button_down_on() && enabled;
            let hovered = response.hovered() && enabled;
            let (fill, stroke, fg) = match (self.kind, enabled) {
                (Kind::Primary, true) => (
                    if pressed {
                        p.accent_pressed
                    } else if hovered {
                        p.accent_hover
                    } else {
                        p.accent
                    },
                    Stroke::NONE,
                    p.on_accent,
                ),
                (Kind::DangerSolid, true) => (
                    if pressed || hovered {
                        p.danger.gamma_multiply(0.88)
                    } else {
                        p.danger
                    },
                    Stroke::NONE,
                    Color32::WHITE,
                ),
                (Kind::Primary | Kind::DangerSolid, false) => (p.subtle, Stroke::NONE, p.faint),
                (Kind::Secondary, _) => (
                    if pressed {
                        p.pressed
                    } else if hovered {
                        p.hover
                    } else {
                        p.surface
                    },
                    Stroke::new(1., p.border_strong),
                    if enabled { p.text } else { p.faint },
                ),
                (Kind::Ghost, _) => (
                    if pressed {
                        p.pressed
                    } else if hovered {
                        p.hover
                    } else {
                        Color32::TRANSPARENT
                    },
                    Stroke::NONE,
                    if !enabled {
                        p.faint
                    } else if hovered {
                        p.text
                    } else {
                        p.text_2
                    },
                ),
                (Kind::Link, _) => (
                    Color32::TRANSPARENT,
                    Stroke::NONE,
                    if !enabled {
                        p.faint
                    } else if hovered {
                        p.accent_hover
                    } else {
                        p.accent
                    },
                ),
                (Kind::Danger, _) => (
                    if hovered {
                        p.danger_soft
                    } else {
                        Color32::TRANSPARENT
                    },
                    Stroke::NONE,
                    if enabled { p.danger } else { p.faint },
                ),
            };
            let radius = if self.small { 7 } else { 8 };
            ui.painter()
                .rect(rect, radius, fill, stroke, StrokeKind::Inside);
            let start = rect.center().x - content_w / 2.;
            if let Some(icon) = self.icon {
                let r = Rect::from_min_size(
                    pos2(start, rect.center().y - icon_size / 2.),
                    vec2(icon_size, icon_size),
                );
                icons::paint(ui.painter(), r, icon, fg);
            }
            let text_pos = pos2(start + icon_w, rect.center().y - galley.size().y / 2.);
            ui.painter().galley(text_pos, galley, fg);
            if self.kind == Kind::Link && hovered {
                let y = text_pos.y + font.size + 3.;
                ui.painter().hline(
                    text_pos.x..=text_pos.x + content_w - icon_w,
                    y,
                    Stroke::new(1., fg.gamma_multiply(0.6)),
                );
            }
            focus_ring(ui, &response, rect, radius as f32);
        }
        let response = match self.tooltip {
            Some(t) => response.on_hover_text(t),
            None => response,
        };
        if enabled {
            response.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

/// Square icon-only ghost button with a tooltip and an optional count badge.
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_button_badge(ui, icon, tooltip, 0, false)
}

pub fn icon_button_badge(
    ui: &mut Ui,
    icon: Icon,
    tooltip: &str,
    badge: usize,
    active: bool,
) -> Response {
    let p = pal(ui);
    let size = 32.;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tooltip));
    if ui.is_rect_visible(rect) {
        let fill = if response.is_pointer_button_down_on() {
            p.pressed
        } else if response.hovered() || active {
            p.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 8, fill);
        let color = if response.hovered() || active {
            p.text
        } else {
            p.text_2
        };
        icons::paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), vec2(18., 18.)),
            icon,
            color,
        );
        if badge > 0 {
            count_badge(ui, pos2(rect.right() - 6., rect.top() + 7.), badge);
        }
        focus_ring(ui, &response, rect, 8.);
    }
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

/// Small accent bubble with a number, centred on `center`.
pub fn count_badge(ui: &Ui, center: Pos2, n: usize) {
    let p = pal(ui);
    let text = if n > 99 { "99+".into() } else { n.to_string() };
    let g = ui.fonts(|f| f.layout_no_wrap(text, bold(10.5), p.on_accent));
    let w = (g.size().x + 8.).max(16.);
    let r = Rect::from_center_size(center, vec2(w, 16.));
    ui.painter().rect_filled(r.expand(1.5), 9, p.bg);
    ui.painter().rect_filled(r, 8, p.accent);
    ui.painter().galley(
        pos2(center.x - g.size().x / 2., center.y - g.size().y / 2.),
        g,
        p.on_accent,
    );
}

/// On/off switch.
pub fn paint_switch(ui: &Ui, rect: Rect, id: Id, on: bool, enabled: bool) {
    let p = pal(ui);
    let t = ui.ctx().animate_bool_responsive(id, on);
    let off = if p.dark { p.raised } else { p.border_strong };
    let mut track = off.lerp_to_gamma(p.accent, t);
    if !enabled {
        track = track.gamma_multiply(0.5);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same((rect.height() / 2.) as u8), track);
    let r = rect.height() / 2. - 2.;
    let x = egui::lerp((rect.left() + r + 2.)..=(rect.right() - r - 2.), t);
    let c = pos2(x, rect.center().y);
    painter.circle_filled(c + vec2(0., 0.7), r + 0.3, Color32::from_black_alpha(36));
    painter.circle_filled(c, r, Color32::WHITE);
}

/// A row of a settings group: title (and description) on the left, `right`
/// laid out right-aligned. Returns the row's response (hover only).
pub fn setting_row<R>(
    ui: &mut Ui,
    title: &str,
    description: Option<&str>,
    right_width: f32,
    right: impl FnOnce(&mut Ui) -> R,
) -> R {
    let p = pal(ui);
    let width = ui.available_width();
    let text_w = (width - right_width - 12.).max(60.);
    let title_g = galley(ui, title, body(13.5), p.text, text_w);
    let desc_g = description.map(|d| {
        let mut job =
            LayoutJob::single_section(d.to_owned(), TextFormat::simple(body(12.), p.muted));
        job.wrap = egui::text::TextWrapping::wrap_at_width(text_w);
        ui.fonts(|f| f.layout_job(job))
    });
    let text_h = title_g.size().y + desc_g.as_ref().map_or(0., |g| g.size().y + 1.);
    let height = (text_h + 20.).max(46.);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let top = rect.center().y - text_h / 2.;
    ui.painter()
        .galley(pos2(rect.left(), top), title_g.clone(), p.text);
    if let Some(g) = desc_g {
        ui.painter()
            .galley(pos2(rect.left(), top + title_g.size().y + 1.), g, p.muted);
    }
    let right_rect = Rect::from_min_max(pos2(rect.right() - right_width, rect.top()), rect.max);
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(right_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        right,
    )
    .inner
}

/// Settings row with a switch; the whole row toggles. Returns true when changed.
pub fn toggle_row(
    ui: &mut Ui,
    value: &mut bool,
    title: &str,
    description: Option<&str>,
    enabled: bool,
) -> bool {
    let p = pal(ui);
    let width = ui.available_width();
    let text_w = width - 64.;
    let title_g = galley(
        ui,
        title,
        body(13.5),
        if enabled { p.text } else { p.faint },
        text_w,
    );
    let desc_g = description.map(|d| {
        let mut job =
            LayoutJob::single_section(d.to_owned(), TextFormat::simple(body(12.), p.muted));
        job.wrap = egui::text::TextWrapping::wrap_at_width(text_w);
        ui.fonts(|f| f.layout_job(job))
    });
    let text_h = title_g.size().y + desc_g.as_ref().map_or(0., |g| g.size().y + 1.);
    let height = (text_h + 20.).max(46.);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
    let changed = response.clicked();
    if changed {
        *value = !*value;
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *value, title));
    if ui.is_rect_visible(rect) {
        let top = rect.center().y - text_h / 2.;
        ui.painter()
            .galley(pos2(rect.left(), top), title_g.clone(), p.text);
        if let Some(g) = desc_g {
            ui.painter()
                .galley(pos2(rect.left(), top + title_g.size().y + 1.), g, p.muted);
        }
        let switch =
            Rect::from_center_size(pos2(rect.right() - 19., rect.center().y), vec2(38., 22.));
        paint_switch(ui, switch, response.id, *value, enabled);
        focus_ring(ui, &response, rect.expand2(vec2(6., -4.)), 8.);
    }
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand);
    }
    changed
}

/// Check box with a label; returns true when changed.
pub fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> bool {
    let p = pal(ui);
    let g = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), body(13.), p.text));
    let size = vec2(18. + 8. + g.size().x, g.size().y.max(20.));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let changed = response.clicked();
    if changed {
        *value = !*value;
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *value, label));
    let bx = Rect::from_center_size(pos2(rect.left() + 9., rect.center().y), vec2(17., 17.));
    let t = ui.ctx().animate_bool_responsive(response.id, *value);
    let painter = ui.painter();
    if t > 0. {
        painter.rect_filled(bx, 5, p.accent.gamma_multiply(t.max(0.3)));
    }
    if t < 1. {
        let border = if response.hovered() {
            p.accent
        } else {
            p.border_strong
        };
        painter.rect_stroke(
            bx,
            5,
            Stroke::new(1.3, border.gamma_multiply(1. - t)),
            StrokeKind::Inside,
        );
    }
    if *value {
        icons::paint(painter, bx.shrink(1.5), Icon::Check, p.on_accent);
    }
    painter.galley(
        pos2(bx.right() + 8., rect.center().y - g.size().y / 2.),
        g,
        p.text,
    );
    focus_ring(ui, &response, bx, 5.);
    changed
}

/// Segmented control of `width`. Returns the index that was clicked, if any.
/// `items` are `(icon, label)`.
pub fn segmented(
    ui: &mut Ui,
    id: &str,
    current: usize,
    items: &[(Option<Icon>, &str)],
    width: f32,
) -> Option<usize> {
    let p = pal(ui);
    let height = 30.;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 9, p.subtle);
    let inner = rect.shrink(2.);
    let w = inner.width() / items.len() as f32;
    let x = ui
        .ctx()
        .animate_value_with_time(Id::new((id, "pos")), current as f32, 0.14);
    let sel = Rect::from_min_size(
        pos2(inner.left() + x * w, inner.top()),
        vec2(w, inner.height()),
    );
    if !p.dark {
        painter.add(
            Shadow {
                offset: [0, 1],
                blur: 3,
                spread: 0,
                color: p.shadow_strong.gamma_multiply(0.5),
            }
            .as_shape(sel, 7),
        );
    }
    painter.rect_filled(sel, 7, p.raised);
    let mut clicked = None;
    for (i, (icon, label)) in items.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(inner.left() + w * i as f32, inner.top()),
            vec2(w, inner.height()),
        );
        let response = ui
            .interact(r, Id::new((id, i)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        response.widget_info(|| {
            WidgetInfo::selected(WidgetType::SelectableLabel, true, i == current, *label)
        });
        if response.clicked() {
            clicked = Some(i);
        }
        focus_ring(ui, &response, r, 7.);
        let selected = i == current;
        let color = if selected || response.hovered() {
            p.text
        } else {
            p.text_2
        };
        let font = if selected { bold(13.) } else { body(13.) };
        let g = ui.fonts(|f| f.layout_no_wrap(label.to_string(), font, color));
        let icon_w = if icon.is_some() { 20. } else { 0. };
        let start = r.center().x - (g.size().x + icon_w) / 2.;
        if let Some(icon) = icon {
            icons::paint(
                &painter,
                Rect::from_center_size(pos2(start + 7., r.center().y), vec2(14., 14.)),
                *icon,
                color,
            );
        }
        painter.galley(
            pos2(start + icon_w, r.center().y - g.size().y / 2.),
            g,
            color,
        );
    }
    clicked
}

/// Thin rounded progress bar.
pub fn progress(ui: &mut Ui, fraction: f32, color: Color32) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.), Sense::hover());
    paint_progress(ui.painter(), rect, fraction, color, p.subtle);
}

pub fn paint_progress(
    painter: &egui::Painter,
    rect: Rect,
    fraction: f32,
    color: Color32,
    track: Color32,
) {
    let radius = CornerRadius::same((rect.height() / 2.) as u8);
    painter.rect_filled(rect, radius, track);
    let f = fraction.clamp(0., 1.);
    if f > 0. {
        let mut fill = rect;
        fill.set_width((rect.width() * f).max(rect.height()));
        painter.rect_filled(fill, radius, color);
    }
}

/// Circle with an icon inside, e.g. transfer directions.
pub fn icon_badge(ui: &Ui, center: Pos2, radius: f32, icon: Icon, fg: Color32, bg: Color32) {
    ui.painter().circle_filled(center, radius, bg);
    let s = radius * 1.0;
    icons::paint(
        ui.painter(),
        Rect::from_center_size(center, vec2(s, s)),
        icon,
        fg,
    );
}

pub fn platform_icon(platform: Platform) -> Icon {
    match platform {
        Platform::Mac => Icon::Laptop,
        _ => Icon::Computer,
    }
}

/// Round device avatar with the platform's icon.
pub fn avatar(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    platform: Platform,
    fg: Color32,
    bg: Color32,
) {
    painter.circle_filled(center, radius, bg);
    let s = radius * 1.05;
    icons::paint(
        painter,
        Rect::from_center_size(center, vec2(s, s)),
        platform_icon(platform),
        fg,
    );
}

pub fn dashed_rounded_rect(painter: &egui::Painter, rect: Rect, radius: f32, stroke: Stroke) {
    let mut pts = icons::rounded_rect_points(rect, radius, 10);
    pts.push(pts[0]);
    painter.extend(Shape::dashed_line(&pts, stroke, 6., 4.));
}

/// Colours for a file-type tile.
fn file_colors(p: &Palette, name: &str, dir: bool) -> (Color32, String) {
    if dir {
        return (Color32::from_rgb(59, 130, 246), String::new());
    }
    let ext = std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    let color = match ext.as_str() {
        "JPG" | "JPEG" | "PNG" | "GIF" | "HEIC" | "WEBP" | "BMP" | "TIFF" | "SVG" | "RAW" => {
            Color32::from_rgb(147, 51, 234)
        }
        "MP4" | "MOV" | "MKV" | "AVI" | "WMV" | "M4V" | "MP3" | "WAV" | "FLAC" | "M4A" | "AAC" => {
            Color32::from_rgb(219, 39, 119)
        }
        "ZIP" | "RAR" | "7Z" | "TAR" | "GZ" | "XZ" | "DMG" | "ISO" | "PKG" | "EXE" | "MSI" => {
            Color32::from_rgb(217, 119, 6)
        }
        "PDF" => Color32::from_rgb(220, 38, 38),
        "DOC" | "DOCX" | "PAGES" | "TXT" | "MD" | "RTF" => p.accent,
        "XLS" | "XLSX" | "CSV" | "NUMBERS" => Color32::from_rgb(22, 163, 74),
        "PPT" | "PPTX" | "KEY" => Color32::from_rgb(234, 88, 12),
        _ => p.text_2,
    };
    (color, ext)
}

/// Small rounded tile showing a file's extension (or a folder), coloured by type.
pub fn file_tile(ui: &Ui, rect: Rect, name: &str, dir: bool) {
    let p = pal(ui);
    let (base, ext) = file_colors(p, name, dir);
    let bg = base.gamma_multiply(if p.dark { 0.26 } else { 0.11 });
    let fg = if p.dark {
        base.lerp_to_gamma(Color32::WHITE, 0.35)
    } else {
        base
    };
    ui.painter().rect_filled(rect, 8, bg);
    if dir || ext.is_empty() || ext.chars().count() > 4 {
        icons::paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), rect.size() * 0.52),
            if dir { Icon::Folder } else { Icon::File },
            fg,
        );
    } else {
        let size = if ext.chars().count() > 3 { 9. } else { 10. };
        ui.painter()
            .text(rect.center(), Align2::CENTER_CENTER, ext, bold(size), fg);
    }
}

/// Label that never wraps (long text is cut with "…").
pub fn truncated(ui: &mut Ui, text: impl Into<RichText>) -> Response {
    ui.add(
        egui::Label::new(text.into())
            .wrap_mode(TextWrapMode::Truncate)
            .selectable(false),
    )
}

/// Item of a context or overflow menu.
pub fn menu_item(ui: &mut Ui, icon: Icon, label: &str, danger: bool) -> Response {
    let p = pal(ui);
    let g = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), body(13.), p.text));
    let width = (g.size().x + 44.).max(ui.available_width()).max(160.);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let hovered = response.hovered();
    let (fg, bg) = match (hovered, danger) {
        (true, true) => (Color32::WHITE, p.danger),
        (true, false) => (p.on_accent, p.accent),
        (false, true) => (p.danger, Color32::TRANSPARENT),
        (false, false) => (p.text, Color32::TRANSPARENT),
    };
    ui.painter().rect_filled(rect, 6, bg);
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(rect.left() + 16., rect.center().y), vec2(15., 15.)),
        icon,
        fg,
    );
    ui.painter().galley(
        pos2(rect.left() + 32., rect.center().y - g.size().y / 2.),
        g,
        fg,
    );
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Right-click menu for a text field: cut, copy, paste and select all.
/// Each item sends the field the same command as the keyboard shortcut, on
/// the next frame (see `run_edit_command`), so the selection, undo and
/// `char_limit` behave exactly as with the keyboard.
pub fn edit_menu(response: &Response, text: &str) {
    let ctx = &response.ctx;
    let id = response.id;
    let saved = id.with("edit-menu-selection");
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    // egui moves the cursor to the pointer on any press, which would drop the
    // selection a right-click is meant to act on: put it back.
    if response.hovered() && ctx.input(|i| i.pointer.button_pressed(PointerButton::Secondary)) {
        if let Some(range) = ctx
            .data(|d| d.get_temp::<Option<CCursorRange>>(saved))
            .flatten()
        {
            state.cursor.set_char_range(Some(range));
            state.clone().store(ctx, id);
        }
    }
    let range = state.cursor.char_range();
    ctx.data_mut(|d| d.insert_temp(saved, range));
    let selected = range.is_some_and(|r| r.primary.index != r.secondary.index);
    response.context_menu(|ui| {
        ui.set_min_width(180.);
        let mut command = None;
        for (label, key, enabled) in [
            ("剪切", Key::X, selected),
            ("复制", Key::C, selected),
            ("粘贴", Key::V, true),
            ("全选", Key::A, !text.is_empty()),
        ] {
            if key == Key::A {
                ui.add_space(2.);
                separator(ui);
                ui.add_space(2.);
            }
            if shortcut_item(ui, label, key, enabled).clicked() {
                command = edit_event(key);
            }
        }
        if let Some(event) = command {
            queue_edit_command(ui.ctx(), Some(id), event);
            ui.close_menu();
        }
    });
}

/// The egui event that ⌘/Ctrl + `key` produces for cut, copy, paste or
/// select all. egui cannot read the clipboard itself: the keyboard shortcut
/// gets the text from egui-winit, the menus from arboard.
pub fn edit_event(key: Key) -> Option<Event> {
    match key {
        Key::X => Some(Event::Cut),
        Key::C => Some(Event::Copy),
        Key::V => {
            let pasted = arboard::Clipboard::new().and_then(|mut c| c.get_text());
            pasted.ok().map(|t| Event::Paste(t.replace("\r\n", "\n")))
        }
        Key::A => Some(Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }),
        _ => None,
    }
}

const EDIT_COMMAND: &str = "edit-menu-command";

/// Sends an edit command to `field`, or to whatever has keyboard focus, at
/// the start of the next frame (see `run_edit_command`).
pub fn queue_edit_command(ctx: &egui::Context, field: Option<Id>, event: Event) {
    ctx.data_mut(|d| d.insert_temp(Id::new(EDIT_COMMAND), Some((field, event))));
    ctx.request_repaint();
}

/// Hands a queued edit command to its field, as if typed.
/// Called at the start of the frame, before any text field is shown.
pub fn run_edit_command(ctx: &egui::Context) {
    let key = Id::new(EDIT_COMMAND);
    let Some((field, event)) = ctx
        .data_mut(|d| d.remove_temp::<Option<(Option<Id>, Event)>>(key))
        .flatten()
    else {
        return;
    };
    if let Some(field) = field {
        ctx.memory_mut(|m| m.request_focus(field));
    }
    ctx.input_mut(|i| i.events.push(event));
}

/// Edit menu row: label on the left, keyboard shortcut on the right.
fn shortcut_item(ui: &mut Ui, label: &str, key: Key, enabled: bool) -> Response {
    let p = pal(ui);
    let shortcut = ui
        .ctx()
        .format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key));
    let width = ui.available_width().max(180.);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.), sense);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    let hovered = enabled && response.hovered();
    let (fg, hint) = match (enabled, hovered) {
        (false, _) => (p.muted, p.muted),
        (true, true) => (p.on_accent, p.on_accent),
        (true, false) => (p.text, p.muted),
    };
    if hovered {
        ui.painter().rect_filled(rect, 6, p.accent);
    }
    let y = rect.center().y;
    ui.painter().text(
        pos2(rect.left() + 12., y),
        Align2::LEFT_CENTER,
        label,
        body(13.),
        fg,
    );
    ui.painter().text(
        pos2(rect.right() - 12., y),
        Align2::RIGHT_CENTER,
        shortcut,
        body(12.),
        hint,
    );
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// On the Mac, Control-click is a right-click (for mice with one button and
/// trackpads without two-finger click). egui only knows real right-clicks,
/// so the Control-clicks are turned into right-clicks before egui sees them.
pub fn control_click_as_right_click(ctx: &egui::Context, raw: &mut egui::RawInput) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let held = Id::new("control-click-held");
    for event in &mut raw.events {
        let Event::PointerButton {
            button,
            pressed,
            modifiers,
            ..
        } = event
        else {
            continue;
        };
        if *button != PointerButton::Primary {
            continue;
        }
        // The release counts as a right-click too even if Control was let
        // go first, so egui never sees a left button that stays down.
        let right = if *pressed {
            let control = modifiers.ctrl && !modifiers.mac_cmd;
            ctx.data_mut(|d| d.insert_temp(held, control));
            control
        } else {
            ctx.data_mut(|d| d.remove_temp::<bool>(held))
                .unwrap_or(false)
        };
        if right {
            *button = PointerButton::Secondary;
        }
    }
}
