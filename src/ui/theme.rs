//! Colours, fonts and the egui style for both light and dark appearance.
use eframe::egui::{
    self, style::ScrollStyle, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Margin, Shadow, Stroke, TextStyle, Theme, Visuals,
};

/// Semantic colours. Every custom-painted element picks its colours from here,
/// so light and dark mode stay consistent.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Window background behind the cards.
    pub bg: Color32,
    /// Cards, dialogs and menus.
    pub surface: Color32,
    /// Quiet fills: segmented-control track, chips, avatars.
    pub subtle: Color32,
    /// The selected segment, raised above the track.
    pub raised: Color32,
    /// Text-field background.
    pub input: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    /// Hairlines between rows of a group.
    pub separator: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub muted: Color32,
    /// Placeholders and disabled text.
    pub faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub success_soft: Color32,
    pub warning: Color32,
    pub warning_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub shadow: Color32,
    pub shadow_strong: Color32,
    pub backdrop: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: rgb(242, 243, 246),
    surface: rgb(255, 255, 255),
    subtle: rgb(236, 238, 242),
    raised: rgb(255, 255, 255),
    input: rgb(246, 247, 249),
    hover: rgb(241, 243, 246),
    pressed: rgb(231, 234, 239),
    border: rgb(226, 229, 234),
    border_strong: rgb(207, 212, 220),
    separator: rgb(236, 238, 242),
    text: rgb(22, 24, 29),
    text_2: rgb(82, 89, 102),
    muted: rgb(128, 135, 148),
    faint: rgb(172, 178, 189),
    accent: rgb(47, 111, 237),
    accent_hover: rgb(36, 97, 219),
    accent_pressed: rgb(29, 83, 192),
    accent_soft: rgb(232, 240, 254),
    on_accent: rgb(255, 255, 255),
    success: rgb(22, 138, 70),
    success_soft: rgb(229, 246, 236),
    warning: rgb(190, 108, 0),
    warning_soft: rgb(254, 243, 222),
    danger: rgb(212, 48, 48),
    danger_soft: rgb(253, 236, 236),
    shadow: Color32::from_black_alpha(10),
    shadow_strong: Color32::from_black_alpha(38),
    backdrop: Color32::from_black_alpha(64),
};

pub const DARK: Palette = Palette {
    dark: true,
    bg: rgb(23, 24, 28),
    surface: rgb(33, 35, 40),
    subtle: rgb(44, 47, 53),
    raised: rgb(64, 68, 77),
    input: rgb(26, 28, 32),
    hover: rgb(44, 47, 53),
    pressed: rgb(53, 56, 63),
    border: rgb(47, 50, 57),
    border_strong: rgb(68, 72, 81),
    separator: rgb(45, 48, 54),
    text: rgb(236, 237, 240),
    text_2: rgb(180, 185, 194),
    muted: rgb(132, 138, 149),
    faint: rgb(94, 99, 109),
    accent: rgb(64, 132, 255),
    accent_hover: rgb(88, 148, 255),
    accent_pressed: rgb(52, 114, 230),
    accent_soft: rgb(31, 45, 71),
    on_accent: rgb(255, 255, 255),
    success: rgb(76, 200, 128),
    success_soft: rgb(25, 51, 37),
    warning: rgb(240, 180, 72),
    warning_soft: rgb(58, 45, 21),
    danger: rgb(245, 112, 112),
    danger_soft: rgb(64, 31, 33),
    shadow: Color32::from_black_alpha(50),
    shadow_strong: Color32::from_black_alpha(110),
    backdrop: Color32::from_black_alpha(130),
};

pub fn palette(visuals: &Visuals) -> &'static Palette {
    if visuals.dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// Palette for the style the `Ui` is currently drawn with.
pub fn pal(ui: &egui::Ui) -> &'static Palette {
    palette(ui.visuals())
}

const BOLD: &str = "bold";

pub fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, bold_family())
}

/// Noto Sans SC as static instances. The upstream variable font renders as
/// Thin in egui (it cannot pick a variation), so `scripts/make_fonts.py`
/// produces a full Regular and a SemiBold subset for headings.
pub(crate) fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "noto-regular".into(),
        FontData::from_static(include_bytes!("../../assets/NotoSansSC-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "noto-semibold".into(),
        FontData::from_static(include_bytes!("../../assets/NotoSansSC-SemiBold.ttf")).into(),
    );
    // Keep egui's emoji / symbol fonts as fallbacks, but let Noto Sans SC draw
    // Latin text as well so mixed Chinese and English reads as one typeface.
    let fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .into_iter()
        .flatten()
        .filter(|name| name.as_str() != "Ubuntu-Light")
        .cloned()
        .collect();
    let mut proportional = vec!["noto-regular".to_owned()];
    proportional.extend(fallbacks.iter().cloned());
    let mut heading = vec!["noto-semibold".to_owned(), "noto-regular".to_owned()];
    heading.extend(fallbacks);
    fonts
        .families
        .insert(FontFamily::Proportional, proportional);
    fonts.families.insert(bold_family(), heading);
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("noto-regular".into());
    fonts
}

pub fn configure(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    // Follow the operating system's light / dark appearance.
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::System);
    ctx.style_mut_of(Theme::Light, |s| apply(s, &LIGHT));
    ctx.style_mut_of(Theme::Dark, |s| apply(s, &DARK));
}

fn apply(style: &mut egui::Style, p: &Palette) {
    style.text_styles = [
        (TextStyle::Small, body(12.)),
        (TextStyle::Body, body(13.5)),
        (TextStyle::Button, body(13.5)),
        (TextStyle::Heading, bold(17.)),
        (TextStyle::Monospace, FontId::monospace(13.)),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8., 8.);
    s.button_padding = egui::vec2(12., 5.);
    s.interact_size = egui::vec2(36., 30.);
    s.icon_width = 16.;
    s.icon_width_inner = 8.;
    s.icon_spacing = 6.;
    s.window_margin = Margin::same(20);
    s.menu_margin = Margin::same(6);
    s.scroll = ScrollStyle::floating();
    s.scroll.floating_width = 6.;
    style.interaction.selectable_labels = false;
    style.interaction.tooltip_delay = 0.4;
    style.animation_time = 0.14;

    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1., p.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_shadow = Shadow {
        offset: [0, 14],
        blur: 40,
        spread: 0,
        color: p.shadow_strong,
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: p.shadow_strong,
    };
    v.menu_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.subtle;
    v.code_bg_color = p.subtle;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.45 } else { 0.22 });
    v.selection.stroke = Stroke::new(1.5, p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);
    v.striped = false;
    v.indent_has_left_vline = false;
    v.override_text_color = None;

    let radius = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1., p.separator);
    w.noninteractive.fg_stroke = Stroke::new(1., p.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill, stroke) in [
        (&mut w.inactive, p.input, p.border),
        (&mut w.hovered, p.input, p.border_strong),
        (&mut w.active, p.input, p.accent),
        (&mut w.open, p.hover, p.border_strong),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1., stroke);
        state.fg_stroke = Stroke::new(1.5, p.text);
        state.corner_radius = radius;
        state.expansion = 0.;
    }
    style.visuals = v;
}
