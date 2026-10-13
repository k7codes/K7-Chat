use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};

use crate::theme::Theme;

/// Tam pencere arka planı: degrade + hareket eden parlak lekeler.
pub fn background(ctx: &egui::Context, theme: &Theme) {
    let t = ctx.input(|i| i.time);
    let size = ctx.content_rect().size();
    let painter = ctx.layer_painter(egui::LayerId::background());

    // Diagonal degrade
    let steps = 40;
    for i in 0..steps {
        let f = i as f32 / steps as f32;
        let a = f * f * (3.0 - 2.0 * f);
        let c = lerp(theme.bg, theme.bg2, a);
        let x0 = size.x * f;
        let x1 = size.x * (i + 1) as f32 / steps as f32 + 1.0;
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(x0, 0.0), Vec2::new(x1 - x0, size.y)),
            CornerRadius::ZERO,
            c,
        );
    }

    // Yumuşak parlak lekeler (sahte radyal degrade)
    blob(&painter, theme, size, t, 0.18, 0.22, theme.accent, 26, 190.0, 0.11);
    blob(&painter, theme, size, t, 0.82, 0.78, theme.accent2, 20, 230.0, 0.07);
    blob(&painter, theme, size, t, 0.62, 0.12, theme.accent2, 13, 150.0, 0.13);
}

fn blob(
    painter: &egui::Painter,
    theme: &Theme,
    size: Vec2,
    t: f64,
    bx: f32,
    by: f32,
    color: Color32,
    alpha: u8,
    radius: f32,
    speed: f32,
) {
    let cx = size.x * bx + (t as f32 * speed * 3.1).sin() * size.x * 0.05;
    let cy = size.y * by + (t as f32 * speed * 2.3 + 1.7).cos() * size.y * 0.06;
    let [r, g, b, _] = color.to_array();
    let rings = 14;
    for i in 0..rings {
        let f = 1.0 - i as f32 / rings as f32;
        let a = (alpha as f32 * f * f * 2.2) as u8;
        painter.circle_filled(Pos2::new(cx, cy), radius * (0.35 + f * 0.65),
            Color32::from_rgba_unmultiplied(r, g, b, a));
    }
    let _ = theme;
}

/// Neon kenarlıklı yumuşak çerçeve.
pub fn glass(theme: &Theme) -> egui::Frame {
    egui::Frame::default()
        .fill(theme.panel)
        .corner_radius(CornerRadius::same(theme.radius as u8))
        .stroke(Stroke::new(1.0, theme.panel_stroke))
        .inner_margin(egui::Margin::same(14))
}

pub fn card(theme: &Theme, pad: f32) -> egui::Frame {
    egui::Frame::default()
        .fill(theme.panel)
        .corner_radius(CornerRadius::same((theme.radius + 4.0) as u8))
        .stroke(Stroke::new(1.0, theme.panel_stroke))
        .inner_margin(egui::Margin::same(pad as i8))
}

/// Yatay degrade dolgu (sahte gradyan).
pub fn gradient_rect(painter: &egui::Painter, rect: Rect, rounding: CornerRadius, a: Color32, b: Color32) {
    let slices = 24;
    let w = rect.width() / slices as f32;
    for i in 0..slices {
        let f = i as f32 / (slices - 1) as f32;
        let c = lerp(a, b, f);
        let x = rect.left() + i as f32 * w;
        let piece = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w + 0.7, rect.height()));
        let r = if i == 0 && i == slices - 1 {
            rounding
        } else if i == 0 {
            CornerRadius {
                nw: rounding.nw,
                sw: rounding.sw,
                ne: 0,
                se: 0,
            }
        } else if i == slices - 1 {
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: rounding.ne,
                se: rounding.se,
            }
        } else {
            CornerRadius::ZERO
        };
        painter.rect_filled(piece, r, c);
    }
}

/// Gölgeli (glow) başlık yazısı.
pub fn glow_text(
    painter: &egui::Painter,
    pos: Pos2,
    text: &str,
    font: FontId,
    fill: Color32,
    glow: Color32,
) {
    let offsets = [
        Vec2::new(-1.5, 0.0),
        Vec2::new(1.5, 0.0),
        Vec2::new(0.0, -1.5),
        Vec2::new(0.0, 1.5),
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(-1.0, 1.0),
        Vec2::new(1.0, -1.0),
    ];
    for off in offsets {
        painter.text(pos + off, egui::Align2::LEFT_CENTER, text, font.clone(), glow);
    }
    painter.text(pos, egui::Align2::LEFT_CENTER, text, font, fill);
}

/// Neon buton: hover'da parlama, basışta hafif küçülme.
pub fn neon_button(ui: &mut egui::Ui, label: &str, theme: &Theme, id: egui::Id) -> egui::Response {
    let desired = Vec2::new(ui.available_width().min(260.0), 40.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let hover = ui.ctx().animate_bool(id.with("hover"), response.hovered());
    let press = if response.is_pointer_button_down_on() { 1.0 } else { 0.0 };
    let press_anim = ui.ctx().animate_value_with_time(id.with("press"), press, 0.08);

    let r = CornerRadius::same((theme.radius - 2.0) as u8);
    let scale = 1.0 - press_anim * 0.03;
    let shrink = Vec2::new(rect.width() * (1.0 - scale) * 0.5, rect.height() * (1.0 - scale) * 0.5);
    let inner = Rect::from_min_max(rect.min + shrink, rect.max - shrink);

    let painter = ui.painter().clone();
    // Dış glow
    let glow_a = (14.0 + hover * 70.0) as u8;
    let [ar, ag, ab, _] = theme.accent.to_array();
    for k in 0..3 {
        painter.rect_stroke(
            inner,
            r,
            Stroke::new(
                3.0 + k as f32 * 2.0 + hover * 2.0,
                Color32::from_rgba_unmultiplied(ar, ag, ab, (glow_a as f32 / (k + 1) as f32) as u8),
            ),
            egui::StrokeKind::Outside,
        );
    }
    // Dolgu
    let c1 = lerp(theme.accent, theme.accent2, 0.0);
    let c2 = lerp(theme.accent, theme.accent2, 0.65 + hover * 0.35);
    gradient_rect(&painter, inner, r, c1, c2);

    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(16.0, FontFamily::Proportional),
        Color32::WHITE,
    );
    let gp = Pos2::new(
        inner.center().x - galley.size().x * 0.5,
        inner.center().y - galley.size().y * 0.5,
    );
    painter.galley(gp, galley, Color32::WHITE);

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Dönen bağlanıyor göstergesi (3 halka).
pub fn spinner(ui: &mut egui::Ui, theme: &Theme, id: egui::Id) {
    let t = ui.input(|i| i.time);
    let size = Vec2::splat(22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().clone();
    let center = rect.center();
    let [r, g, b, _] = theme.accent2.to_array();
    for i in 0..3 {
        let phase = (t as f32 * 3.0 - i as f32 * 0.45).rem_euclid(1.0);
        let a = ((1.0 - phase) * 200.0) as u8;
        let y = center.y + (phase * std::f32::consts::PI).sin() * 3.0;
        painter.circle_filled(
            Pos2::new(center.x + (i as f32 - 1.0) * 8.0, y),
            3.2,
            Color32::from_rgba_unmultiplied(r, g, b, a),
        );
    }
    let _ = id;
}

/// Nabız atan LED daire.
pub fn led(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, t: f64, pulse: bool) {
    let [r, g, b, _] = color.to_array();
    let breathe = if pulse {
        0.55 + 0.45 * (t as f32 * 2.4).sin()
    } else {
        1.0
    };
    for k in 0..3 {
        let f = 1.0 - k as f32 / 3.0;
        painter.circle_filled(center, radius * (1.0 + k as f32 * 0.9),
            Color32::from_rgba_unmultiplied(r, g, b, ((36.0 * f * breathe) as u8).min(90)));
    }
    painter.circle_filled(center, radius, color);
}

/// "yazıyor..." zıplayan üç nokta.
pub fn typing_dots(painter: &egui::Painter, pos: Pos2, t: f64, color: Color32) {
    for i in 0..3 {
        let phase = t as f32 * 4.0 - i as f32 * 0.9;
        let y = pos.y + phase.sin().abs() * -4.0;
        painter.circle_filled(Pos2::new(pos.x + i as f32 * 7.0, y), 2.6, color);
    }
}

/// Yuvarlak "chip" (istatistik etiketi).
pub fn chip(ui: &mut egui::Ui, text: &str, theme: &Theme, accent: Color32) {
    let font = FontId::new(11.5, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, theme.dim);
    let size = galley.size() + Vec2::new(16.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().clone();
    let [r, g, b, _] = accent.to_array();
    painter.rect_filled(
        rect,
        CornerRadius::same(99),
        Color32::from_rgba_unmultiplied(r, g, b, 26),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(99),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(r, g, b, 90)),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        theme.dim,
    );
}

/// Odalar/DM listesi için satır: aktifken parlak çizgi + okunmamış rozeti.
pub fn list_row(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    unread: u32,
    theme: &Theme,
    id: egui::Id,
    leading: Option<Color32>,
) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    let t = ui.input(|i| i.time);
    let hover = ui.ctx().animate_bool(id.with("h"), response.hovered());
    let act = ui.ctx().animate_bool(id.with("a"), active);
    let painter = ui.painter().clone();

    let r = CornerRadius::same(9);
    if act > 0.01 {
        let [ar, ag, ab, _] = theme.accent.to_array();
        painter.rect_filled(
            rect,
            r,
            Color32::from_rgba_unmultiplied(ar, ag, ab, (18.0 * act) as u8),
        );
    }
    if hover > 0.01 && !active {
        painter.rect_filled(
            rect,
            r,
            Color32::from_rgba_unmultiplied(255, 255, 255, (10.0 * hover) as u8),
        );
    }
    if active {
        let bar = Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + 6.0),
            Vec2::new(3.0, rect.height() - 12.0),
        );
        let pulse = 0.7 + 0.3 * (t as f32 * 2.0).sin();
        gradient_rect(
            &painter,
            bar,
            CornerRadius::same(2),
            theme.accent,
            lerp(theme.accent, theme.accent2, pulse),
        );
    }

    let mut x = rect.left() + 12.0;
    if let Some(c) = leading {
        painter.circle_filled(Pos2::new(x + 3.0, rect.center().y), 3.0, c);
        x += 12.0;
    }
    let color = if active { theme.text } else { lerp(theme.dim, theme.text, hover) };
    painter.text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::new(14.0, FontFamily::Proportional),
        color,
    );

    if unread > 0 {
        let txt = if unread > 99 { "99+".to_string() } else { unread.to_string() };
        let font = FontId::new(11.0, FontFamily::Proportional);
        let g = ui.painter().layout_no_wrap(txt.clone(), font, Color32::WHITE);
        let w = g.size().x + 12.0;
        let badge = Rect::from_min_size(
            Pos2::new(rect.right() - w - 8.0, rect.center().y - 9.0),
            Vec2::new(w, 18.0),
        );
        painter.rect_filled(badge, CornerRadius::same(99), theme.danger);
        painter.galley(
            Pos2::new(
                badge.center().x - g.size().x * 0.5,
                badge.center().y - g.size().y * 0.5,
            ),
            g,
            Color32::WHITE,
        );
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Küçük başlık metni.
pub fn section_label(ui: &mut egui::Ui, text: &str, theme: &Theme) {
    ui.add_space(8.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    ui.painter().text(
        Pos2::new(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text.to_uppercase(),
        FontId::new(10.5, FontFamily::Proportional),
        lerp(theme.dim, theme.accent, 0.45),
    );
    ui.add_space(2.0);
}

/// Bağlı/kesik LED + etiket.
pub fn status_line(ui: &mut egui::Ui, label: &str, color: Color32, pulse: bool, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let t = ui.input(|i| i.time);
    let painter = ui.painter().clone();
    led(&painter, Pos2::new(rect.left() + 6.0, rect.center().y), 3.5, color, t, pulse);
    painter.text(
        Pos2::new(rect.left() + 18.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::new(12.5, FontFamily::Proportional),
        theme.dim,
    );
}

pub fn input_theme(ui: &mut egui::Ui, theme: &Theme) {
    ui.style_mut().visuals.extreme_bg_color = lerp(theme.bg, Color32::BLACK, 0.35);
    ui.style_mut().visuals.faint_bg_color = lerp(theme.bg, Color32::BLACK, 0.2);
    ui.style_mut().visuals.selection.bg_fill = theme.accent;
    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = lerp(theme.panel, theme.accent, 0.12);
    ui.style_mut().visuals.widgets.hovered.weak_bg_fill = lerp(theme.panel, theme.accent, 0.22);
    ui.style_mut().visuals.widgets.active.weak_bg_fill = lerp(theme.panel, theme.accent, 0.32);
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [r1, g1, b1, a1] = a.to_array();
    let [r2, g2, b2, a2] = b.to_array();
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_unmultiplied(f(r1, r2), f(g1, g2), f(b1, b2), f(a1, a2))
}

/// Sola hizalı, vurgulu etiket.
pub fn label(ui: &mut egui::Ui, text: &str, size: f32, color: Color32) {
    ui.label(RichText::new(text).size(size).color(color));
}
