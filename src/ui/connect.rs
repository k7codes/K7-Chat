use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Sense, Stroke, Vec2,
};

use crate::{
    java_bridge::{Bridge, Status as JavaStatus},
    state::{ClientUi, ConnState},
    theme::Theme,
    ui::widgets,
};

pub enum ConnectAction {
    Connect,
    None,
}

#[allow(clippy::too_many_arguments)]
pub fn ui(
    ui: &mut egui::Ui,
    state: &mut ClientUi,
    bridge: &Bridge,
    theme: &Theme,
) -> ConnectAction {
    let mut action = ConnectAction::None;
    let now = ui.input(|i| i.time);

    let total_w = ui.available_width();
    let card_w = total_w.min(430.0);
    ui.add_space((ui.available_height().max(60.0) - 360.0) * 0.3);

    let inner_w = card_w - 48.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(card_w, 364.0), Sense::hover());
    let painter = ui.painter().clone();
    let r = CornerRadius::same((theme.radius + 6.0) as u8);

    // Dış neon glow + cam kart
    let [ar, ag, ab, _] = theme.accent.to_array();
    let breathe = 0.75 + 0.25 * (now as f32 * 1.6).sin();
    for k in 0..3 {
        painter.rect_stroke(
            rect.expand(2.0 + k as f32 * 3.0),
            r,
            Stroke::new(
                1.5,
                Color32::from_rgba_unmultiplied(ar, ag, ab, ((30.0 * breathe / (k + 1) as f32) as u8).max(6)),
            ),
            egui::StrokeKind::Outside,
        );
    }
    painter.rect_filled(rect, r, theme.panel);
    painter.rect_stroke(rect, r, Stroke::new(1.0, theme.panel_stroke), egui::StrokeKind::Inside);

    // Başlık
    widgets::glow_text(
        &painter,
        Pos2::new(rect.left() + 24.0, rect.top() + 42.0),
        "RustChat",
        FontId::new(30.0, FontFamily::Proportional),
        theme.text,
        theme.accent,
    );
    painter.text(
        Pos2::new(rect.left() + 24.0, rect.top() + 70.0),
        egui::Align2::LEFT_CENTER,
        "sunucuya bağlan",
        FontId::new(13.0, FontFamily::Proportional),
        theme.dim,
    );

    // Java rozeti (sağ üst)
    let (jlabel, jcolor) = match bridge.status() {
        JavaStatus::Active => ("Java modülü: aktif", theme.ok),
        JavaStatus::Starting => ("Java: başlatılıyor…", theme.accent2),
        JavaStatus::Failed(_) => ("Java: kapalı", theme.dim),
        JavaStatus::Disabled => ("Java: derlenmedi", theme.dim),
    };
    let badge_txt = format!("☕ {jlabel}");
    let badge_font = FontId::new(11.5, FontFamily::Proportional);
    let g = ui.painter().layout_no_wrap(badge_txt, badge_font, jcolor);
    let badge = egui::Rect::from_min_size(
        Pos2::new(rect.right() - g.size().x - 20.0, rect.top() + 22.0),
        g.size() + Vec2::new(14.0, 8.0),
    );
    let [jr, jg, jb, _] = jcolor.to_array();
    painter.rect_filled(
        badge,
        CornerRadius::same(99),
        Color32::from_rgba_unmultiplied(jr, jg, jb, 24),
    );
    painter.galley(
        Pos2::new(badge.left() + 7.0, badge.center().y - g.size().y * 0.5),
        g,
        jcolor,
    );

    // İçerik alanı
    let content = egui::Rect::from_min_size(
        Pos2::new(rect.left() + 24.0, rect.top() + 96.0),
        Vec2::new(inner_w, 244.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    widgets::input_theme(&mut child, theme);
    let ui = &mut child;

    field_label(ui, "SUNUCU ADRESİ", theme);
    ui.horizontal(|ui| {
        ui.set_width(inner_w);
        let ip_w = inner_w * 0.62;
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.server_ip)
                .desired_width(ip_w)
                .font(FontId::new(15.0, FontFamily::Proportional)),
        );
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if resp.lost_focus() && enter {
            action = ConnectAction::Connect;
        }
        ui.add_space(8.0);
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.server_port)
                .hint_text("port")
                .desired_width(inner_w - ip_w - 8.0)
                .font(FontId::new(15.0, FontFamily::Proportional)),
        );
        if resp.lost_focus() && enter {
            action = ConnectAction::Connect;
        }
    });

    ui.add_space(12.0);
    field_label(ui, "KULLANICI ADI", theme);
    let name_resp = ui.add(
        egui::TextEdit::singleline(&mut state.username)
            .hint_text("örn. k7")
            .desired_width(inner_w)
            .font(FontId::new(15.0, FontFamily::Proportional)),
    );

    ui.add_space(18.0);

    let connecting = matches!(state.conn, ConnState::Connecting);
    let can_connect = !state.server_ip.trim().is_empty()
        && !state.server_port.trim().is_empty()
        && !state.username.trim().is_empty();
    let enabled = can_connect && !connecting;

    let (rect_b, resp_b) = ui.allocate_exact_size(Vec2::new(inner_w, 44.0), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool(egui::Id::new("conn_btn_h"), resp_b.hovered());
    let pulse = if connecting {
        0.55 + 0.45 * (now as f32 * 3.0).sin()
    } else {
        1.0
    };
    let btn_color = if connecting { theme.accent2 } else { theme.accent };

    if enabled || connecting {
        let [cr, cg, cb, _] = btn_color.to_array();
        for k in 0..3 {
            painter.rect_stroke(
                rect_b.expand(2.0 + k as f32 * 2.0 + hover * 2.0),
                CornerRadius::same(12),
                Stroke::new(
                    1.4,
                    Color32::from_rgba_unmultiplied(
                        cr,
                        cg,
                        cb,
                        ((30.0 * pulse / (k + 1) as f32) as u8).max(5),
                    ),
                ),
                egui::StrokeKind::Outside,
            );
        }
        let (c1, c2) = if connecting {
            (
                widgets::lerp(theme.accent2, Color32::BLACK, 0.5),
                theme.accent2,
            )
        } else {
            (
                theme.accent,
                widgets::lerp(theme.accent, theme.accent2, 0.55 + hover * 0.45),
            )
        };
        widgets::gradient_rect(&painter, rect_b, CornerRadius::same(12), c1, c2);
    } else {
        painter.rect_filled(rect_b, CornerRadius::same(12), theme.bg2);
        painter.rect_stroke(
            rect_b,
            CornerRadius::same(12),
            Stroke::new(1.0, theme.panel_stroke),
            egui::StrokeKind::Inside,
        );
    }

    let label_color = if enabled || connecting {
        Color32::WHITE
    } else {
        theme.dim
    };
    if connecting {
        painter.text(
            Pos2::new(rect_b.center().x - 14.0, rect_b.center().y),
            egui::Align2::CENTER_CENTER,
            "Bağlanıyor",
            FontId::new(16.0, FontFamily::Proportional),
            label_color,
        );
        // Yüzen üç nokta
        let [dr, dg, db, _] = theme.accent2.to_array();
        for i in 0..3 {
            let phase = now as f32 * 5.0 - i as f32 * 0.6;
            let y = rect_b.center().y + phase.sin().abs() * -5.0;
            let a = (140.0 + 110.0 * phase.sin().abs()) as u8;
            painter.circle_filled(
                Pos2::new(rect_b.center().x + 52.0 + i as f32 * 11.0, y),
                3.4,
                Color32::from_rgba_unmultiplied(dr, dg, db, a),
            );
        }
    } else {
        painter.text(
            rect_b.center(),
            egui::Align2::CENTER_CENTER,
            "Bağlan",
            FontId::new(16.0, FontFamily::Proportional),
            label_color,
        );
    }

    if resp_b.hovered() && enabled {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if (resp_b.clicked() && enabled) || (name_resp.lost_focus() && enabled) {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) || resp_b.clicked() {
            action = ConnectAction::Connect;
        }
    }

    // Alt durum satırı
    let status_pos = Pos2::new(rect.left() + 24.0, rect.bottom() - 24.0);
    if let Some((text, at)) = &state.error_flash {
        let age = at.elapsed().as_secs_f64();
        if age < 6.0 {
            let fade = if age > 5.0 { (6.0 - age) as f32 } else { 1.0 };
            painter.text(
                status_pos,
                egui::Align2::LEFT_CENTER,
                format!("⚠ {text}"),
                FontId::new(12.5, FontFamily::Proportional),
                fade_color(theme.danger, fade),
            );
        }
    } else if let Some(reason) = &state.disconnect_reason {
        painter.text(
            status_pos,
            egui::Align2::LEFT_CENTER,
            format!("⚠ {reason}"),
            FontId::new(12.5, FontFamily::Proportional),
            theme.danger,
        );
    } else {
        painter.text(
            status_pos,
            egui::Align2::LEFT_CENTER,
            "İpucu: \"Sunucu\" sekmesinden kendi sunucunu başlat",
            FontId::new(12.0, FontFamily::Proportional),
            theme.dim,
        );
    }

    action
}

fn field_label(ui: &mut egui::Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .size(10.5)
            .color(widgets::lerp(theme.dim, theme.accent, 0.5)),
    );
    ui.add_space(4.0);
}

fn fade_color(c: Color32, k: f32) -> Color32 {
    let [r, g, b, a] = c.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * k) as u8)
}
