//! Vector icons drawn with the egui painter, plus the raster app / tray icon.
//!
//! Icons are described on a unit square and scaled into the target rect, so
//! they stay crisp at any size and follow the palette in light and dark mode.
use eframe::egui::{epaint::PathStroke, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Computer,
    Upload,
    File,
    Folder,
    Close,
    Check,
    ArrowUp,
    ArrowDown,
    Copy,
    Warning,
    Plus,
    /// "!" without a circle, for use on a filled badge.
    Bang,
    /// "i" without a circle, for use on a filled badge.
    InfoMark,
    Laptop,
    Gear,
    /// Up and down arrows: transfers.
    Transfers,
    Back,
    /// Lines of text.
    Text,
    /// Speech bubble: a received text message.
    Message,
    Paste,
    /// Arrow leaving a box: open a link or a file.
    External,
    Shield,
    ShieldCheck,
    Trash,
    Search,
}

struct Pen<'a> {
    painter: &'a Painter,
    rect: Rect,
    stroke: Stroke,
    color: Color32,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(
            self.rect.left() + x * self.rect.width(),
            self.rect.top() + y * self.rect.height(),
        )
    }
    fn s(&self, v: f32) -> f32 {
        v * self.rect.width()
    }
    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.painter
            .add(Shape::line(pts, PathStroke::from(self.stroke)));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.painter
            .add(Shape::closed_line(pts, PathStroke::from(self.stroke)));
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.painter
            .circle_stroke(self.p(x, y), self.s(r), self.stroke);
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.painter
            .circle_filled(self.p(x, y), self.s(r), self.color);
    }
    fn rounded(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) {
        let rect = Rect::from_min_max(self.p(x0, y0), self.p(x1, y1));
        self.painter.rect_stroke(
            rect,
            self.s(r),
            self.stroke,
            eframe::egui::StrokeKind::Middle,
        );
    }
}

/// Paint `icon` inside `rect` (should be square) with the given colour.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let width = (rect.width() * 0.085).clamp(1.2, 2.6);
    let pen = Pen {
        painter,
        rect,
        stroke: Stroke::new(width, color),
        color,
    };
    match icon {
        Icon::Computer => {
            pen.rounded(0.12, 0.16, 0.88, 0.66, 0.06);
            pen.line(&[(0.5, 0.66), (0.5, 0.82)]);
            pen.line(&[(0.32, 0.84), (0.68, 0.84)]);
        }
        Icon::Upload => {
            pen.line(&[(0.16, 0.6), (0.16, 0.82), (0.84, 0.82), (0.84, 0.6)]);
            pen.line(&[(0.5, 0.64), (0.5, 0.16)]);
            pen.line(&[(0.3, 0.36), (0.5, 0.16), (0.7, 0.36)]);
        }
        Icon::File => {
            pen.closed(&[
                (0.24, 0.1),
                (0.6, 0.1),
                (0.78, 0.28),
                (0.78, 0.9),
                (0.24, 0.9),
            ]);
            pen.line(&[(0.58, 0.1), (0.58, 0.3), (0.78, 0.3)]);
        }
        Icon::Folder => {
            pen.closed(&[
                (0.1, 0.24),
                (0.4, 0.24),
                (0.5, 0.34),
                (0.9, 0.34),
                (0.9, 0.8),
                (0.1, 0.8),
            ]);
        }
        Icon::Close => {
            pen.line(&[(0.28, 0.28), (0.72, 0.72)]);
            pen.line(&[(0.72, 0.28), (0.28, 0.72)]);
        }
        Icon::Check => pen.line(&[(0.22, 0.52), (0.42, 0.72), (0.8, 0.3)]),
        Icon::Plus => {
            pen.line(&[(0.5, 0.2), (0.5, 0.8)]);
            pen.line(&[(0.2, 0.5), (0.8, 0.5)]);
        }
        Icon::ArrowUp => {
            pen.line(&[(0.5, 0.82), (0.5, 0.18)]);
            pen.line(&[(0.26, 0.42), (0.5, 0.18), (0.74, 0.42)]);
        }
        Icon::ArrowDown => {
            pen.line(&[(0.5, 0.18), (0.5, 0.82)]);
            pen.line(&[(0.26, 0.58), (0.5, 0.82), (0.74, 0.58)]);
        }
        Icon::Copy => {
            pen.rounded(0.36, 0.36, 0.86, 0.86, 0.1);
            pen.line(&[
                (0.64, 0.24),
                (0.64, 0.14),
                (0.14, 0.14),
                (0.14, 0.64),
                (0.24, 0.64),
            ]);
        }
        Icon::Warning => {
            pen.closed(&[(0.5, 0.12), (0.9, 0.84), (0.1, 0.84)]);
            pen.line(&[(0.5, 0.4), (0.5, 0.6)]);
            pen.dot(0.5, 0.71, 0.05);
        }
        Icon::Bang => {
            pen.line(&[(0.5, 0.22), (0.5, 0.56)]);
            pen.dot(0.5, 0.76, 0.075);
        }
        Icon::InfoMark => {
            pen.dot(0.5, 0.25, 0.075);
            pen.line(&[(0.5, 0.44), (0.5, 0.78)]);
        }
        Icon::Laptop => {
            pen.rounded(0.2, 0.24, 0.8, 0.66, 0.05);
            pen.line(&[(0.08, 0.78), (0.92, 0.78)]);
        }
        Icon::Gear => {
            let mut pts = vec![];
            for k in 0..8 {
                let a = k as f32 * PI / 4.;
                for (r, da) in [(0.33, -0.27), (0.44, -0.14), (0.44, 0.14), (0.33, 0.27)] {
                    pts.push(pen.p(0.5 + r * (a + da).cos(), 0.5 + r * (a + da).sin()));
                }
            }
            painter.add(Shape::closed_line(pts, PathStroke::from(pen.stroke)));
            pen.circle(0.5, 0.5, 0.13);
        }
        Icon::Transfers => {
            pen.line(&[(0.34, 0.8), (0.34, 0.2)]);
            pen.line(&[(0.18, 0.36), (0.34, 0.2), (0.5, 0.36)]);
            pen.line(&[(0.66, 0.2), (0.66, 0.8)]);
            pen.line(&[(0.5, 0.64), (0.66, 0.8), (0.82, 0.64)]);
        }
        Icon::Back => pen.line(&[(0.62, 0.18), (0.32, 0.5), (0.62, 0.82)]),
        Icon::Text => {
            pen.line(&[(0.16, 0.28), (0.84, 0.28)]);
            pen.line(&[(0.16, 0.5), (0.84, 0.5)]);
            pen.line(&[(0.16, 0.72), (0.58, 0.72)]);
        }
        Icon::Message => {
            pen.closed(&[
                (0.14, 0.2),
                (0.86, 0.2),
                (0.86, 0.7),
                (0.46, 0.7),
                (0.28, 0.86),
                (0.28, 0.7),
                (0.14, 0.7),
            ]);
            pen.line(&[(0.3, 0.38), (0.7, 0.38)]);
            pen.line(&[(0.3, 0.53), (0.58, 0.53)]);
        }
        Icon::Paste => {
            pen.rounded(0.2, 0.18, 0.8, 0.9, 0.08);
            pen.rounded(0.36, 0.1, 0.64, 0.26, 0.05);
            pen.line(&[(0.34, 0.5), (0.66, 0.5)]);
            pen.line(&[(0.34, 0.68), (0.56, 0.68)]);
        }
        Icon::External => {
            pen.line(&[(0.46, 0.2), (0.2, 0.2), (0.2, 0.8), (0.8, 0.8), (0.8, 0.54)]);
            pen.line(&[(0.48, 0.52), (0.84, 0.16)]);
            pen.line(&[(0.58, 0.16), (0.84, 0.16), (0.84, 0.42)]);
        }
        Icon::Shield | Icon::ShieldCheck => {
            pen.closed(&[
                (0.5, 0.1),
                (0.82, 0.22),
                (0.8, 0.5),
                (0.7, 0.7),
                (0.5, 0.88),
                (0.3, 0.7),
                (0.2, 0.5),
                (0.18, 0.22),
            ]);
            if icon == Icon::ShieldCheck {
                pen.line(&[(0.36, 0.5), (0.47, 0.61), (0.66, 0.4)]);
            }
        }
        Icon::Trash => {
            pen.line(&[(0.16, 0.26), (0.84, 0.26)]);
            pen.line(&[(0.4, 0.26), (0.42, 0.14), (0.58, 0.14), (0.6, 0.26)]);
            pen.line(&[(0.26, 0.26), (0.3, 0.86), (0.7, 0.86), (0.74, 0.26)]);
        }
        Icon::Search => {
            pen.circle(0.44, 0.44, 0.26);
            pen.line(&[(0.63, 0.63), (0.84, 0.84)]);
        }
    }
}

/// Stroke segments of the two arrows in the app icon, on the unit square.
/// `scripts/make_icons.py` uses the same geometry for the packaged icons.
const ARROWS: [[(f32, f32); 2]; 6] = [
    [(0.24, 0.36), (0.75, 0.36)],
    [(0.58, 0.2), (0.76, 0.36)],
    [(0.76, 0.36), (0.58, 0.52)],
    [(0.76, 0.64), (0.25, 0.64)],
    [(0.42, 0.48), (0.24, 0.64)],
    [(0.24, 0.64), (0.42, 0.8)],
];
const ARROW_WIDTH: f32 = 0.085;
const TILE_TOP: [f32; 3] = [76., 141., 255.];
const TILE_BOTTOM: [f32; 3] = [29., 84., 216.];

fn tile_color(t: f32) -> Color32 {
    let c = |i: usize| (TILE_TOP[i] + (TILE_BOTTOM[i] - TILE_TOP[i]) * t).round() as u8;
    Color32::from_rgb(c(0), c(1), c(2))
}

/// The app logo: rounded tile with a vertical gradient and two opposing arrows.
pub fn paint_logo(painter: &Painter, rect: Rect) {
    let radius = rect.width() * 0.26;
    // Anti-aliased base, then the (unfeathered) gradient slightly inset.
    painter.rect_filled(rect, radius, tile_color(0.5));
    let inner = rect.shrink(0.75);
    painter.add(Shape::mesh(gradient_rounded_rect(
        inner,
        radius - 0.75,
        tile_color(0.),
        tile_color(1.),
    )));
    let unit = rect.width();
    let stroke = Stroke::new(unit * ARROW_WIDTH, Color32::WHITE);
    let p = |(x, y): (f32, f32)| pos2(rect.left() + x * unit, rect.top() + y * unit);
    for [a, b] in ARROWS {
        painter.line_segment([p(a), p(b)], stroke);
        // Round the joints / ends to match the raster icon.
        painter.circle_filled(p(a), stroke.width / 2., Color32::WHITE);
        painter.circle_filled(p(b), stroke.width / 2., Color32::WHITE);
    }
}

fn gradient_rounded_rect(
    rect: Rect,
    radius: f32,
    top: Color32,
    bottom: Color32,
) -> eframe::egui::Mesh {
    use eframe::egui::Mesh;
    let mut mesh = Mesh::default();
    let outline = rounded_rect_points(rect, radius, 8);
    let color_at = |y: f32| {
        let t = ((y - rect.top()) / rect.height()).clamp(0., 1.);
        top.lerp_to_gamma(bottom, t)
    };
    let center = rect.center();
    mesh.colored_vertex(center, color_at(center.y));
    for p in &outline {
        mesh.colored_vertex(*p, color_at(p.y));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    mesh
}

/// Points along the outline of a rounded rectangle, clockwise from the top-left
/// straight edge. The first point is not repeated at the end.
pub fn rounded_rect_points(rect: Rect, radius: f32, steps: usize) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.).min(rect.height() / 2.);
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -0.5 * PI),
        (pos2(rect.right() - r, rect.bottom() - r), 0.),
        (pos2(rect.left() + r, rect.bottom() - r), 0.5 * PI),
        (pos2(rect.left() + r, rect.top() + r), PI),
    ];
    let mut pts = Vec::with_capacity(4 * (steps + 1));
    for (c, start) in corners {
        for i in 0..=steps {
            let a = start + 0.5 * PI * i as f32 / steps as f32;
            pts.push(c + r * vec2(a.cos(), a.sin()));
        }
    }
    pts
}

/// Whether a point of the unit square is inside the icon tile and inside the
/// white arrows. Shared by the window, tray and packaged icons.
fn icon_sample(fx: f32, fy: f32) -> (bool, bool) {
    let dx = (fx - 0.5).abs();
    let dy = (fy - 0.5).abs();
    let (half, r) = (0.47, 0.16);
    let inside = dx < half
        && dy < half
        && (dx < half - r || dy < half - r || (dx - (half - r)).hypot(dy - (half - r)) < r);
    let distance = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (vx, vy) = (bx - ax, by - ay);
        let t = (((fx - ax) * vx + (fy - ay) * vy) / (vx * vx + vy * vy)).clamp(0., 1.);
        (fx - ax - t * vx).hypot(fy - ay - t * vy)
    };
    let arrow = ARROWS
        .iter()
        .any(|&[a, b]| distance(a, b) < ARROW_WIDTH / 2.);
    (inside, arrow)
}

/// RGBA pixels of the colour app icon (window icon, Windows tray icon).
pub fn icon(size: usize) -> Vec<u8> {
    raster(size, false)
}

/// Black arrows on transparent, for the macOS menu bar (template image).
pub fn tray_glyph(size: usize) -> Vec<u8> {
    raster(size, true)
}

fn raster(size: usize, glyph_only: bool) -> Vec<u8> {
    const SS: usize = 4; // 4×4 supersampling for smooth edges
    let mut bytes = vec![0; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let (mut tile, mut arrow) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let mut fx = (x as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let mut fy = (y as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if glyph_only {
                        // Enlarge the arrows so they fill the menu-bar slot.
                        fx = 0.5 + (fx - 0.5) * 0.74;
                        fy = 0.5 + (fy - 0.5) * 0.74;
                    }
                    let (i, a) = icon_sample(fx, fy);
                    tile += i as u32;
                    arrow += (a && (i || glyph_only)) as u32;
                }
            }
            let n = (SS * SS) as f32;
            let (tile, arrow) = (tile as f32 / n, arrow as f32 / n);
            let p = (y * size + x) * 4;
            if glyph_only {
                bytes[p..p + 4].copy_from_slice(&[0, 0, 0, (arrow * 255.).round() as u8]);
            } else if tile > 0. {
                let base = tile_color((y as f32 + 0.5) / size as f32);
                let mix = arrow / tile;
                let c = |v: u8| (v as f32 + (255. - v as f32) * mix).round() as u8;
                bytes[p..p + 4].copy_from_slice(&[
                    c(base.r()),
                    c(base.g()),
                    c(base.b()),
                    (tile * 255.).round() as u8,
                ]);
            }
        }
    }
    bytes
}
