use std::time::Instant;

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2,
};

use crate::{
    java_bridge::Bridge,
    protocol::Scope,
    state::ClientUi,
    theme::Theme,
    ui::widgets::{self, lerp},
};

pub enum Action {
    Send(String),
    SwitchScope(Scope),
    StartDm(String),
    None,
}

#[derive(Default)]
pub struct Analysis {
    pub text: String,
    pub json: serde_json::Value,
    pub hint: String,
}

impl Analysis {
    pub fn refresh(&mut self, bridge: &Bridge, input: &str) {
        if self.text == input {
            return;
        }
        self.text = input.to_string();
        let raw = bridge.process(input);
        self.json = serde_json::from_str(&raw)
            .unwrap_or_else(|_| crate::java_bridge::fallback_process(input));
        self.hint = if input.trim_start().starts_with('/') {
            bridge.hint(input)
        } else {
            String::new()
        };
    }
}

const EMOJIS: &[&str] = &[
    "😀", "😂", "🤣", "😊", "😍", "🥰", "😘", "😎", "🤩", "🥳",
    "🤔", "🤨", "😐", "😴", "🤤", "😋", "🤪", "😇", "🙃", "😅",
    "😭", "😤", "😡", "🤯", "😳", "🥺", "😱", "😨", "🤗", "🤭",
    "👍", "👎", "👏", "🙏", "💪", "🫶", "🤝", "✌️", "🤞", "👌",
    "❤️", "🔥", "✨", "💯", "🎉", "🚀", "⭐", "🌈", "🍀", "🐱",
    "🐶", "🦊", "🐼", "🍕", "☕", "🍺", "🎮", "🎧", "💻", "🦀",
    "🐙", "🏆", "💡", "🎯", "✅", "❌", "⚠️", "💬", "👀", "🌙",
];

pub fn ui(
    ui: &mut egui::Ui,
    state: &mut ClientUi,
    bridge: &Bridge,
    theme: &Theme,
    emoji_open: &mut bool,
    analysis: &mut Analysis,
    started_typing_at: &mut Option<Instant>,
    focus_input: &mut bool,
) -> Vec<Action> {
    let mut actions = Vec::new();
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());

    let side_w = 228.0;
    let side_rect = Rect::from_min_max(full.min, Pos2::new(full.min.x + side_w, full.max.y));
    let chat_rect = Rect::from_min_max(
        Pos2::new(full.min.x + side_w + 14.0, full.min.y + 2.0),
        Pos2::new(full.max.x, full.max.y - 2.0),
    );

    // ---- Sol panel ----
    {
        let mut side = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(side_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        sidebar(&mut side, state, theme, &mut actions);
    }

    // ---- Sohbet ----
    {
        let mut chat = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(chat_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        chat_panel(
            &mut chat,
            state,
            bridge,
            theme,
            emoji_open,
            analysis,
            started_typing_at,
            focus_input,
            &mut actions,
        );
    }

    actions
}

fn sidebar(ui: &mut egui::Ui, state: &mut ClientUi, theme: &Theme, actions: &mut Vec<Action>) {
    let w = ui.available_width();

    // Kendi kartı
    let (me_rect, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
    let p = ui.painter().clone();
    let r = CornerRadius::same(11);
    p.rect_filled(me_rect, r, lerp(theme.panel, theme.accent, 0.10));
    p.rect_stroke(
        me_rect,
        r,
        Stroke::new(1.0, lerp(theme.panel_stroke, theme.accent, 0.4)),
        egui::StrokeKind::Inside,
    );
    let t = ui.input(|i| i.time);
    widgets::led(
        &p,
        Pos2::new(me_rect.left() + 16.0, me_rect.center().y),
        4.0,
        theme.ok,
        t,
        true,
    );
    p.text(
        Pos2::new(me_rect.left() + 30.0, me_rect.center().y - 7.0),
        egui::Align2::LEFT_CENTER,
        state.self_name.clone(),
        FontId::new(14.5, FontFamily::Proportional),
        theme.text,
    );
    p.text(
        Pos2::new(me_rect.left() + 30.0, me_rect.center().y + 9.0),
        egui::Align2::LEFT_CENTER,
        "çevrimiçi",
        FontId::new(10.5, FontFamily::Proportional),
        theme.ok,
    );
    ui.add_space(10.0);

    widgets::section_label(ui, "Odalar", theme);
    let rooms = state.rooms.clone();
    for room in &rooms {
        let scope = Scope::Room { room: room.clone() };
        let active = state.active.key() == scope.key();
        let unread = state.unread.get(&scope.key()).copied().unwrap_or(0);
        let label = format!("#{room}");
        let resp = widgets::list_row(
            ui,
            &label,
            active,
            unread,
            theme,
            egui::Id::new(("room", room)),
            Some(lerp(theme.accent2, theme.accent, 0.3)),
        );
        if resp.clicked() {
            actions.push(Action::SwitchScope(scope));
        }
    }

    // DM listesi (bilinen konuşmalar)
    let mut dms: Vec<String> = state
        .convs
        .keys()
        .filter(|k| k.starts_with('@'))
        .map(|k| k[1..].to_string())
        .collect();
    dms.sort();
    if !dms.is_empty() {
        widgets::section_label(ui, "Sohbetler", theme);
        for peer in &dms {
            let scope = Scope::Dm { peer: peer.clone() };
            let active = state.active.key() == scope.key();
            let unread = state.unread.get(&scope.key()).copied().unwrap_or(0);
            let resp = widgets::list_row(
                ui,
                &format!("@{peer}"),
                active,
                unread,
                theme,
                egui::Id::new(("dm", peer.clone())),
                Some(if state.users.iter().any(|u| u == peer) {
                    theme.ok
                } else {
                    theme.dim
                }),
            );
            if resp.clicked() {
                actions.push(Action::SwitchScope(scope));
            }
        }
    }

    widgets::section_label(ui, &format!("Çevrimiçi — {}", state.users.len()), theme);
    let users = state.users.clone();
    for user in &users {
        let resp = widgets::list_row(
            ui,
            user,
            false,
            0,
            theme,
            egui::Id::new(("user", user.clone())),
            Some(theme.ok),
        );
        if resp.clicked() && user != &state.self_name {
            actions.push(Action::StartDm(user.clone()));
        }
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Java modülü ile zenginleştirilmiş")
            .size(10.0)
            .color(lerp(theme.dim, theme.accent, 0.35)),
    );
}

#[allow(clippy::too_many_arguments)]
fn chat_panel(
    ui: &mut egui::Ui,
    state: &mut ClientUi,
    bridge: &Bridge,
    theme: &Theme,
    emoji_open: &mut bool,
    analysis: &mut Analysis,
    started_typing_at: &mut Option<Instant>,
    focus_input: &mut bool,
    actions: &mut Vec<Action>,
) {
    let w = ui.available_width();
    let h = ui.available_height();
    let now = ui.input(|i| i.time);

    // ---- Başlık ----
    let (head, _) = ui.allocate_exact_size(Vec2::new(w, 50.0), Sense::hover());
    let p = ui.painter().clone();
    let title = state.active.display();
    widgets::glow_text(
        &p,
        Pos2::new(head.left(), head.top() + 18.0),
        &title,
        FontId::new(21.0, FontFamily::Proportional),
        theme.text,
        theme.accent,
    );
    let sub = match &state.active {
        Scope::Room { room } => {
            let n = state.users.len();
            format!("#{room} · {n} kişi çevrimiçi")
        }
        Scope::Dm { peer } => {
            if state.users.iter().any(|u| u == peer) {
                format!("@{peer} · özel konuşma")
            } else {
                format!("@{peer} · çevrimdışı")
            }
        }
    };
    p.text(
        Pos2::new(head.left(), head.top() + 38.0),
        egui::Align2::LEFT_CENTER,
        sub,
        FontId::new(12.0, FontFamily::Proportional),
        theme.dim,
    );
    // başlık altı çizgi
    let grad_w = (head.width() * 0.35) * (0.6 + 0.4 * (now as f32 * 1.2).sin().abs());
    widgets::gradient_rect(
        &p,
        Rect::from_min_size(Pos2::new(head.left(), head.bottom() - 2.0), Vec2::new(grad_w, 2.0)),
        CornerRadius::ZERO,
        theme.accent,
        theme.accent2,
    );
    p.rect_filled(
        Rect::from_min_size(Pos2::new(head.left() + grad_w, head.bottom() - 2.0), Vec2::new(w - grad_w, 1.0)),
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(255, 255, 255, 14),
    );

    // ---- Alt yükseklikler ----
    let emoji_h = if *emoji_open { 148.0 } else { 0.0 };
    analysis.refresh(bridge, &state.input);
    let analysis_h = if state.input.trim().is_empty() { 0.0 } else { 26.0 };
    let typing_h = 20.0;
    let input_h = 46.0;
    let bottom = emoji_h + analysis_h + typing_h + input_h + 30.0;
    let msgs_h = (h - 50.0 - bottom).max(80.0);

    // ---- Mesajlar ----
    let (msg_rect, _) = ui.allocate_exact_size(Vec2::new(w, msgs_h), Sense::hover());
    {
        let mut msg_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(msg_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        messages(&mut msg_ui, state, theme, now);
    }

    // ---- Yazıyor ----
    let (typ_rect, _) = ui.allocate_exact_size(Vec2::new(w, typing_h), Sense::hover());
    {
        let typers = state.typing_display(&state.active);
        if !typers.is_empty() {
            let p = ui.painter().clone();
            let mut x = typ_rect.left() + 4.0;
            widgets::typing_dots(&p, Pos2::new(x, typ_rect.center().y), now, theme.accent2);
            x += 28.0;
            let names = if typers.len() == 1 {
                typers[0].clone()
            } else {
                format!("{} ve {} kişi", typers.len(), typers.join(", "))
            };
            p.text(
                Pos2::new(x, typ_rect.center().y),
                egui::Align2::LEFT_CENTER,
                format!("{names} yazıyor…"),
                FontId::new(12.0, FontFamily::Proportional),
                lerp(theme.dim, theme.accent2, 0.5),
            );
        }
    }

    // ---- Analiz şeridi (Java) ----
    if analysis_h > 0.0 {
        let (a_rect, _) = ui.allocate_exact_size(Vec2::new(w, analysis_h), Sense::hover());
        let mut chips: Vec<String> = Vec::new();
        if analysis.json["kind"] == "command" {
            if !analysis.hint.is_empty() {
                chips.push(format!("▸ {}", analysis.hint));
            }
        } else {
            let s = &analysis.json["stats"];
            chips.push(format!("{} kelime", s["words"]));
            chips.push(format!("{} karakter", s["chars"]));
            if s["links"].as_array().is_some_and(|a| !a.is_empty()) {
                chips.push(format!("🔗 {} link", s["links"].as_array().map(|a| a.len()).unwrap_or(0)));
            }
            if s["mentions"].as_array().is_some_and(|a| !a.is_empty()) {
                chips.push(format!("@{}", s["mentions"].as_array().unwrap()[0]));
            }
            if s["hashtags"].as_array().is_some_and(|a| !a.is_empty()) {
                chips.push(format!("#{}", s["hashtags"].as_array().unwrap()[0]));
            }
            if s["emojis"].as_u64().unwrap_or(0) > 0 {
                chips.push(format!("{} emoji", s["emojis"]));
            }
            chips.push("☕ java".into());
        }
        let mut chip_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(a_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        for (i, c) in chips.iter().enumerate() {
            widgets::chip(
                &mut chip_ui,
                c,
                theme,
                if i % 2 == 0 { theme.accent } else { theme.accent2 },
            );
            chip_ui.add_space(6.0);
        }
    }

    // ---- Emoji seçici ----
    if *emoji_open {
        let (e_rect, _) = ui.allocate_exact_size(Vec2::new(w, emoji_h), Sense::hover());
        let p = ui.painter().clone();
        p.rect_filled(e_rect, CornerRadius::same(12), theme.panel);
        p.rect_stroke(
            e_rect,
            CornerRadius::same(12),
            Stroke::new(1.0, theme.panel_stroke),
            egui::StrokeKind::Inside,
        );
        let mut eu = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(e_rect.shrink(10.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        eu.horizontal_wrapped(|ui| {
            for e in EMOJIS {
                let r = ui.add(egui::Label::new(egui::RichText::new(*e).size(19.0)).sense(Sense::click()));
                if r.clicked() {
                    state.input.push_str(e);
                }
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
        });
        let close = Rect::from_min_max(
            Pos2::new(e_rect.right() - 30.0, e_rect.top() + 6.0),
            Pos2::new(e_rect.right() - 6.0, e_rect.top() + 26.0),
        );
        p.text(
            close.center(),
            egui::Align2::CENTER_CENTER,
            "✕",
            FontId::new(14.0, FontFamily::Proportional),
            theme.dim,
        );
        if ui.interact(close, egui::Id::new("emoji_close"), Sense::click()).clicked() {
            *emoji_open = false;
        }
    }

    // ---- Giriş satırı ----
    let (in_rect, _) = ui.allocate_exact_size(Vec2::new(w, input_h), Sense::hover());
    let mut in_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(in_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    widgets::input_theme(&mut in_ui, theme);

    // Emoji butonu
    let emoji_btn = Rect::from_min_size(
        Pos2::new(in_rect.left(), in_rect.top() + 3.0),
        Vec2::new(40.0, 40.0),
    );
    let er = ui.interact(emoji_btn, egui::Id::new("emoji_btn"), Sense::click());
    let p = ui.painter().clone();
    let e_hover = ui
        .ctx()
        .animate_bool(egui::Id::new("emoji_h"), er.hovered() || *emoji_open);
    p.rect_filled(
        emoji_btn,
        CornerRadius::same(11),
        lerp(theme.panel, theme.accent, 0.10 + e_hover * 0.18),
    );
    p.rect_stroke(
        emoji_btn,
        CornerRadius::same(11),
        Stroke::new(
            1.0,
            lerp(theme.panel_stroke, theme.accent, e_hover),
        ),
        egui::StrokeKind::Inside,
    );
    p.text(
        emoji_btn.center(),
        egui::Align2::CENTER_CENTER,
        "😊",
        FontId::new(18.0, FontFamily::Proportional),
        Color32::WHITE,
    );
    if er.clicked() {
        *emoji_open = !*emoji_open;
    }
    if er.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // Metin alanı
    let te_x = emoji_btn.right() + 8.0;
    let te_rect = Rect::from_min_max(
        Pos2::new(te_x, in_rect.top() + 3.0),
        Pos2::new(in_rect.right() - 104.0, in_rect.bottom() - 3.0),
    );
    let te = egui::TextEdit::singleline(&mut state.input)
        .hint_text("Mesaj yaz…  (/help)")
        .desired_width(te_rect.width())
        .font(FontId::new(15.0, FontFamily::Proportional));
    let mut te_ui = in_ui.new_child(
        egui::UiBuilder::new()
            .max_rect(te_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let te_resp = te_ui.add(te);
    if *focus_input {
        te_resp.request_focus();
        *focus_input = false;
    }

    // Gönder butonu
    let send_rect = Rect::from_min_size(
        Pos2::new(in_rect.right() - 94.0, in_rect.top() + 3.0),
        Vec2::new(94.0, 40.0),
    );
    let sr = ui.interact(send_rect, egui::Id::new("send_btn"), Sense::click());
    let can_send = !state.input.trim().is_empty();
    let s_hover = ui
        .ctx()
        .animate_bool(egui::Id::new("send_h"), sr.hovered()) as f32;
    if can_send {
        for k in 0..2 {
            p.rect_stroke(
                send_rect.expand(2.0 + k as f32 * 2.0 + s_hover * 2.0),
                CornerRadius::same(11),
                Stroke::new(
                    1.4,
                    Color32::from_rgba_unmultiplied(
                        theme.accent2.r(),
                        theme.accent2.g(),
                        theme.accent2.b(),
                        ((34.0 - k as f32 * 10.0) * (0.7 + 0.3 * s_hover)) as u8,
                    ),
                ),
                egui::StrokeKind::Outside,
            );
        }
        widgets::gradient_rect(
            &p,
            send_rect,
            CornerRadius::same(11),
            theme.accent,
            lerp(theme.accent, theme.accent2, 0.5 + s_hover * 0.5),
        );
    } else {
        p.rect_filled(send_rect, CornerRadius::same(11), lerp(theme.panel, theme.bg2, 0.5));
        p.rect_stroke(
            send_rect,
            CornerRadius::same(11),
            Stroke::new(1.0, theme.panel_stroke),
            egui::StrokeKind::Inside,
        );
    }
    p.text(
        send_rect.center(),
        egui::Align2::CENTER_CENTER,
        "Gönder ➤",
        FontId::new(14.0, FontFamily::Proportional),
        if can_send { Color32::WHITE } else { theme.dim },
    );
    if sr.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // Enter / tıklama ile gönder
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let should_send = (sr.clicked() || (enter && te_resp.has_focus())) && can_send;
    if should_send {
        let text = std::mem::take(&mut state.input);
        *emoji_open = false;
        *started_typing_at = None;
        actions.push(Action::Send(text));
    }
}

fn messages(ui: &mut egui::Ui, state: &ClientUi, theme: &Theme, now: f64) {
    let avail = ui.available_width();
    let msgs: Vec<&crate::state::ShownMsg> = state.conv(&state.active).iter().collect();
    let self_name = state.self_name.clone();

    egui::ScrollArea::vertical()
        .id_salt("msg_scroll")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(avail);
            let mut prev_from: Option<String> = None;
            let mut prev_ts: i64 = 0;
            for sm in &msgs {
                let own = sm.msg.from == self_name && !sm.msg.system;
                let grouped =
                    prev_from.as_deref() == Some(sm.msg.from.as_str())
                        && sm.msg.ts - prev_ts < 300
                        && !sm.msg.system;
                draw_message(ui, sm, state, theme, avail, now, own, grouped);
                prev_from = Some(sm.msg.from.clone());
                prev_ts = sm.msg.ts;
            }
            ui.add_space(4.0);
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_message(
    ui: &mut egui::Ui,
    sm: &crate::state::ShownMsg,
    state: &ClientUi,
    theme: &Theme,
    avail: f32,
    now: f64,
    own: bool,
    grouped: bool,
) {
    let m = &sm.msg;

    // Giriş animasyonu
    let age = (now - sm.shown_at) as f32;
    let p = (age / 0.38).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - p).powi(3);
    if p <= 0.0 {
        ui.add_space(6.0);
        return;
    }
    let alpha = ease;
    let slide = (1.0 - ease) * 30.0 * if own { 1.0 } else { -1.0 };
    let fade = |c: Color32| -> Color32 {
        let [r, g, b, a] = c.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * alpha) as u8)
    };

    // Sistem mesajı
    if m.system {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, 24.0), Sense::hover());
        let p = ui.painter().clone();
        let cy = rect.center().y;
        let text = format!("· {} ·", m.text);
        let g = ui.painter().layout_no_wrap(
            text.clone(),
            FontId::new(11.5, FontFamily::Proportional),
            fade(theme.dim),
        );
        let tw = g.size().x;
        let cx = rect.center().x - tw * 0.5;
        p.line_segment(
            [Pos2::new(rect.left() + 10.0, cy), Pos2::new(cx - 10.0, cy)],
            Stroke::new(1.0, fade(Color32::from_rgba_unmultiplied(255, 255, 255, 26))),
        );
        p.line_segment(
            [Pos2::new(cx + tw + 10.0, cy), Pos2::new(rect.right() - 10.0, cy)],
            Stroke::new(1.0, fade(Color32::from_rgba_unmultiplied(255, 255, 255, 26))),
        );
        p.galley(Pos2::new(cx, cy - g.size().y * 0.5), g, fade(theme.dim));
        return;
    }

    // /me eylemi
    if m.action {
        let text = format!("{} {}", m.from, m.text);
        let wrap_w = (avail * 0.8).min(640.0);
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = wrap_w;
        job.append(
            &text,
            0.0,
            egui::text::TextFormat {
                font_id: FontId::new(13.5, FontFamily::Proportional),
                color: fade(user_color(&m.from)),
                italics: true,
                ..Default::default()
            },
        );
        let galley = ui.painter().layout_job(job);
        let h = galley.size().y + 10.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, h + 4.0), Sense::hover());
        let p = ui.painter().clone();
        let x = rect.center().x - galley.size().x * 0.5 + slide;
        p.galley(Pos2::new(x, rect.top() + 5.0), galley, fade(theme.dim));
        return;
    }

    // Baloncuk geometrisi
    let pad_x = 13.0;
    let pad_y = 9.0;
    let wrap_w = (avail * 0.60).min(560.0) - pad_x * 2.0;
    let header_h = if grouped { 0.0 } else { 17.0 };

    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_w;
    let text_color = if own {
        fade(Color32::from_rgb(255, 255, 255))
    } else {
        fade(theme.text)
    };
    job.append(
        &m.text,
        0.0,
        egui::text::TextFormat {
            font_id: FontId::new(14.5, FontFamily::Proportional),
            color: text_color,
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let bw = galley.size().x.max(48.0) + pad_x * 2.0;
    let bh = galley.size().y + pad_y * 2.0 + header_h;
    let gap = if grouped { 3.0 } else { 9.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, bh + gap), Sense::hover());
    let p = ui.painter().clone();

    let bx = if own {
        rect.right() - bw + slide
    } else {
        rect.left() + slide
    };
    let bubble = Rect::from_min_size(Pos2::new(bx, rect.top()), Vec2::new(bw, bh));
    let r = CornerRadius::same(theme.radius as u8);

    // Baloncuk dolgusu
    if own {
        widgets::gradient_rect(
            &p,
            bubble,
            r,
            fade(lerp(theme.bubble_me, theme.accent, 0.35)),
            fade(theme.bubble_me),
        );
        p.rect_stroke(
            bubble,
            r,
            Stroke::new(1.0, fade(lerp(theme.accent2, theme.bubble_me, 0.4))),
            egui::StrokeKind::Inside,
        );
    } else {
        p.rect_filled(bubble, r, fade(theme.bubble_other));
        p.rect_stroke(
            bubble,
            r,
            Stroke::new(1.0, fade(lerp(theme.panel_stroke, theme.bubble_other, 0.35))),
            egui::StrokeKind::Inside,
        );
    }

    let text_top = bubble.top() + pad_y + header_h;
    if !grouped && !own {
        // İsim + saat
        let name_col = fade(user_color(&m.from));
        p.text(
            Pos2::new(bubble.left() + pad_x, bubble.top() + pad_y + 6.0),
            egui::Align2::LEFT_CENTER,
            m.from.clone(),
            FontId::new(11.5, FontFamily::Proportional),
            name_col,
        );
        let name_w = ui
            .painter()
            .layout_no_wrap(
                m.from.clone(),
                FontId::new(11.5, FontFamily::Proportional),
                name_col,
            )
            .size()
            .x;
        p.text(
            Pos2::new(bubble.left() + pad_x + name_w + 8.0, bubble.top() + pad_y + 6.0),
            egui::Align2::LEFT_CENTER,
            m.time_string(),
            FontId::new(10.5, FontFamily::Proportional),
            fade(theme.dim),
        );
    }

    p.galley(Pos2::new(bubble.left() + pad_x, text_top), galley, text_color);

    // Kendi mesajında saat + teslim durumu (baloncuğun solunda, dışarıda)
    if own {
        let read = state.is_read(&state.active, m.id, true);
        let status = if read { "✓✓" } else { "✓" };
        let label = format!("{}  {}", m.time_string(), status);
        p.text(
            Pos2::new(bubble.left() - 10.0, bubble.center().y),
            egui::Align2::RIGHT_CENTER,
            label,
            FontId::new(10.5, FontFamily::Proportional),
            fade(if read { theme.ok } else { theme.dim }),
        );
    } else if grouped {
        p.text(
            Pos2::new(bubble.right() - pad_x, bubble.bottom() - pad_y + 2.0),
            egui::Align2::RIGHT_BOTTOM,
            m.time_string(),
            FontId::new(9.5, FontFamily::Proportional),
            fade(Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
        );
    }
}

/// Kullanıcı adından türetilen renk (HSL).
pub fn user_color(name: &str) -> Color32 {
    let mut h: u32 = 2166136261;
    for b in name.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32;
    hsl(hue, 0.62, 0.62)
}

fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color32::from_rgb(
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}
