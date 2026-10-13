use std::time::Instant;

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
};

use crate::{
    protocol::{LogLevel, ServerEvent},
    server::ServerHandle,
    theme::Theme,
    ui::widgets::{self, lerp},
};

pub enum ServerAction {
    Start,
    Stop,
    None,
}

pub struct ServerUi {
    pub port: String,
    pub logs: Vec<(LogLevel, String, String)>,
    pub running: bool,
    pub port_actual: Option<u16>,
    pub started_at: Option<Instant>,
}

impl Default for ServerUi {
    fn default() -> Self {
        Self {
            port: "5000".into(),
            logs: Vec::new(),
            running: false,
            port_actual: None,
            started_at: None,
        }
    }
}

impl ServerUi {
    pub fn drain(&mut self, handle: &ServerHandle) {
        while let Ok(ev) = handle.events.try_recv() {
            match ev {
                ServerEvent::Started { port } => {
                    self.running = true;
                    self.port_actual = Some(port);
                    self.started_at = Some(Instant::now());
                }
                ServerEvent::Stopped => {
                    self.running = false;
                    self.port_actual = None;
                    self.started_at = None;
                }
                ServerEvent::Log { level, text } => {
                    let ts = chrono::Local::now().format("%H:%M:%S").to_string();
                    self.logs.push((level, text, ts));
                    if self.logs.len() > 600 {
                        let cut = self.logs.len() - 600;
                        self.logs.drain(..cut);
                    }
                }
            }
        }
    }
}

pub fn ui(ui: &mut egui::Ui, state: &mut ServerUi, theme: &Theme) -> ServerAction {
    let mut action = ServerAction::None;
    let now = ui.input(|i| i.time);
    let w = ui.available_width();
    let h = ui.available_height();
    let start = ui.cursor().left_top();
    ui.allocate_rect(Rect::from_min_size(start, Vec2::new(w, h)), Sense::hover());

    let card_w = w.min(720.0);
    let card_h = h.min(520.0);
    let origin = Pos2::new(
        start.x + (w - card_w) * 0.5,
        start.y + (h - card_h) * 0.32,
    );
    let card = Rect::from_min_size(origin, Vec2::new(card_w, card_h));
    let p = ui.painter().clone();
    let r = CornerRadius::same((theme.radius + 6.0) as u8);

    let [ar, ag, ab, _] = theme.accent.to_array();
    let pulse = if state.running {
        0.7 + 0.3 * (now as f32 * 2.2).sin()
    } else {
        0.45
    };
    for k in 0..3 {
        p.rect_stroke(
            card.expand(2.0 + k as f32 * 3.0),
            r,
            Stroke::new(
                1.5,
                Color32::from_rgba_unmultiplied(ar, ag, ab, ((30.0 * pulse / (k + 1) as f32) as u8).max(6)),
            ),
            egui::StrokeKind::Outside,
        );
    }
    p.rect_filled(card, r, theme.panel);
    p.rect_stroke(card, r, Stroke::new(1.0, theme.panel_stroke), egui::StrokeKind::Inside);

    // Başlık
    widgets::glow_text(
        &p,
        Pos2::new(card.left() + 24.0, card.top() + 40.0),
        "Sunucu",
        FontId::new(27.0, FontFamily::Proportional),
        theme.text,
        theme.accent,
    );
    let status_text = if state.running {
        match state.port_actual {
            Some(port) => format!("0.0.0.0:{port} adresinde dinliyor"),
            None => "başlatıldı".into(),
        }
    } else {
        "durduruldu".into()
    };
    p.text(
        Pos2::new(card.left() + 24.0, card.top() + 66.0),
        egui::Align2::LEFT_CENTER,
        status_text,
        FontId::new(13.0, FontFamily::Proportional),
        if state.running { theme.ok } else { theme.dim },
    );
    widgets::led(
        &p,
        Pos2::new(card.right() - 40.0, card.top() + 38.0),
        5.0,
        if state.running { theme.ok } else { theme.danger },
        now,
        state.running,
    );
    if state.running {
        if let Some(at) = state.started_at {
            let secs = at.elapsed().as_secs();
            p.text(
                Pos2::new(card.right() - 24.0, card.top() + 62.0),
                egui::Align2::RIGHT_CENTER,
                format!("{}:{:02}", secs / 60, secs % 60),
                FontId::new(11.5, FontFamily::Proportional),
                theme.dim,
            );
        }
    }

    // Port + başlat/durdur
    let row_y = card.top() + 88.0;
    let port_label_pos = Pos2::new(card.left() + 24.0, row_y + 14.0);
    p.text(
        port_label_pos,
        egui::Align2::LEFT_CENTER,
        "PORT",
        FontId::new(10.5, FontFamily::Proportional),
        lerp(theme.dim, theme.accent, 0.5),
    );

    let port_rect = Rect::from_min_size(
        Pos2::new(card.left() + 24.0, row_y + 26.0),
        Vec2::new(130.0, 40.0),
    );
    let mut port_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(port_rect)
            .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
    );
    widgets::input_theme(&mut port_ui, theme);
    let te = egui::TextEdit::singleline(&mut state.port)
        .desired_width(130.0)
        .font(FontId::new(15.0, FontFamily::Proportional));
    let _ = port_ui.add(te).on_hover_text("Dinlenecek port");

    let btn_rect = Rect::from_min_size(
        Pos2::new(card.left() + 168.0, row_y + 26.0),
        Vec2::new(170.0, 40.0),
    );
    let br = ui.interact(btn_rect, egui::Id::new("srv_btn"), Sense::click());
    let b_hover = ui.ctx().animate_bool(egui::Id::new("srv_h"), br.hovered());
    let label = if state.running { "Durdur" } else { "Başlat" };
    let base = if state.running { theme.danger } else { theme.ok };
    let [br_, bg_, bb_, _] = base.to_array();
    for k in 0..2 {
        p.rect_stroke(
            btn_rect.expand(2.0 + k as f32 * 2.0 + b_hover * 2.0),
            CornerRadius::same(11),
            Stroke::new(
                1.4,
                Color32::from_rgba_unmultiplied(
                    br_,
                    bg_,
                    bb_,
                    ((34.0 - k as f32 * 10.0) * (0.7 + 0.3 * b_hover)) as u8,
                ),
            ),
            egui::StrokeKind::Outside,
        );
    }
    widgets::gradient_rect(
        &p,
        btn_rect,
        CornerRadius::same(11),
        lerp(base, theme.bg, 0.25),
        lerp(base, Color32::WHITE, 0.1 + b_hover * 0.15),
    );
    p.text(
        btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(15.0, FontFamily::Proportional),
        Color32::WHITE,
    );
    if br.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if br.clicked() {
        action = if state.running {
            ServerAction::Stop
        } else {
            ServerAction::Start
        };
    }

    // Küçük ipucu
    p.text(
        Pos2::new(card.left() + 360.0, row_y + 46.0),
        egui::Align2::LEFT_CENTER,
        "İstemci sekmesinden 127.0.0.1 ile bağlan",
        FontId::new(12.0, FontFamily::Proportional),
        theme.dim,
    );

    // Log konsolu
    let log_rect = Rect::from_min_size(
        Pos2::new(card.left() + 24.0, card.top() + 152.0),
        Vec2::new(card.width() - 48.0, card.height() - 176.0),
    );
    p.rect_filled(log_rect, CornerRadius::same(10), lerp(theme.bg, Color32::BLACK, 0.5));
    p.rect_stroke(
        log_rect,
        CornerRadius::same(10),
        Stroke::new(1.0, theme.panel_stroke),
        egui::StrokeKind::Inside,
    );
    p.text(
        Pos2::new(log_rect.left() + 12.0, log_rect.top() + 14.0),
        egui::Align2::LEFT_CENTER,
        "GÜNLÜK",
        FontId::new(10.0, FontFamily::Proportional),
        lerp(theme.dim, theme.accent, 0.5),
    );

    let inner = Rect::from_min_size(
        Pos2::new(log_rect.left() + 8.0, log_rect.top() + 26.0),
        Vec2::new(log_rect.width() - 16.0, log_rect.height() - 34.0),
    );
    let mut log_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    if state.logs.is_empty() {
        log_ui.label(
            egui::RichText::new("— henüz kayıt yok —")
                .size(12.0)
                .color(theme.dim),
        );
    }
    egui::ScrollArea::vertical()
        .id_salt("srv_log")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(&mut log_ui, |ui| {
            for (level, text, ts) in &state.logs {
                let color = match level {
                    LogLevel::Info => lerp(theme.text, theme.dim, 0.35),
                    LogLevel::Warn => Color32::from_rgb(240, 190, 80),
                    LogLevel::Error => theme.danger,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("[{ts}] "))
                            .size(12.0)
                            .monospace()
                            .color(theme.dim),
                    );
                    ui.label(
                        egui::RichText::new(text)
                            .size(12.5)
                            .monospace()
                            .color(color),
                    );
                });
            }
        });

    action
}
