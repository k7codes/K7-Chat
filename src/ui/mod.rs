use std::time::Instant;

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use crate::{
    java_bridge::{self, Bridge, Status as JavaStatus},
    net::{self, Client},
    protocol::{ClientMsg, Scope, clean_name},
    server::{self, ServerHandle},
    state::{apply_event, build_outgoing, ClientUi, ConnState, Tab},
    theme::{ThemeState, themes},
};

use self::{
    chat::Analysis,
    serverpanel::{ServerAction, ServerUi},
};

pub mod widgets;
mod chat;
mod connect;
mod serverpanel;

pub struct App {
    tab: Tab,
    theme: ThemeState,
    bridge: Bridge,
    server: Option<ServerHandle>,
    server_ui: ServerUi,
    client: Option<Client>,
    state: ClientUi,
    emoji_open: bool,
    analysis: Analysis,
    started_typing_at: Option<Instant>,
    focus_input: bool,
    frame_time: f64,
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            tab: Tab::Client,
            theme: ThemeState::default(),
            bridge: Bridge::new(),
            server: None,
            server_ui: ServerUi::default(),
            client: None,
            state: ClientUi::default(),
            emoji_open: false,
            analysis: Analysis::default(),
            started_typing_at: None,
            focus_input: false,
            frame_time: 0.0,
        }
    }

    fn start_connect(&mut self) {
        let name = clean_name(&self.state.username);
        if name.is_empty() {
            self.state.set_error("Kullanıcı adı gerekli".into());
            return;
        }
        let Ok(port) = self.state.server_port.trim().parse::<u16>() else {
            self.state.set_error("Geçersiz port".into());
            return;
        };
        let ip = self.state.server_ip.trim().to_string();
        let addr = format!("{ip}:{port}");

        let client = net::connect(addr);
        client.send(ClientMsg::Hello { name });
        self.client = Some(client);
        self.state.conn = ConnState::Connecting;
        self.state.disconnect_reason = None;
        self.state.error_flash = None;
        self.state.convs.clear();
        self.state.unread.clear();
        self.state.reads.clear();
        self.state.typing.clear();
        self.state.just_connected = false;
        self.state.active = Scope::Room {
            room: crate::protocol::DEFAULT_ROOMS[0].to_string(),
        };
    }

    fn send_raw(&self, msg: ClientMsg) {
        if let Some(c) = &self.client {
            c.send(msg);
        }
    }

    fn send_text(&mut self, text: &str) {
        let raw = self.bridge.process(text);
        let parsed = serde_json::from_str(&raw)
            .unwrap_or_else(|_| java_bridge::fallback_process(text));
        let outs = build_outgoing(&mut self.state, &parsed, self.frame_time);
        for o in outs {
            self.send_raw(o);
        }
        // Yazıyor bildirimini kapat
        if self.state.last_sent_typing {
            let scope = self.state.active.clone();
            self.state.last_sent_typing = false;
            self.send_raw(ClientMsg::Typing { scope, typing: false });
        }
        // quit komutu bağlantı düşürdüyse istemciyi kapat
        if self.state.conn == ConnState::Disconnected {
            self.client.take();
        }
    }

    fn switch_scope(&mut self, scope: Scope) {
        if self.state.last_sent_typing {
            let old = self.state.active.clone();
            self.state.last_sent_typing = false;
            self.send_raw(ClientMsg::Typing {
                scope: old,
                typing: false,
            });
        }
        self.state.activate(scope.clone(), self.frame_time);
        if let Some(last) = self.state.conv(&scope).last() {
            let id = last.msg.id;
            self.send_raw(ClientMsg::Read { scope, up_to: id });
        }
    }

    fn start_dm(&mut self, peer: String) {
        self.send_raw(ClientMsg::OpenDm { peer: peer.clone() });
        self.switch_scope(Scope::Dm { peer });
    }

    fn typing_tick(&mut self) {
        if self.state.conn != ConnState::Connected {
            return;
        }
        let non_empty = !self.state.input.trim().is_empty();
        if non_empty && !self.state.last_sent_typing {
            let scope = self.state.active.clone();
            self.state.last_sent_typing = true;
            self.state.last_typing_sent = Instant::now();
            self.send_raw(ClientMsg::Typing { scope, typing: true });
        } else if self.state.last_sent_typing
            && self.state.last_typing_sent.elapsed().as_secs() >= 3
        {
            let scope = self.state.active.clone();
            self.state.last_typing_sent = Instant::now();
            self.send_raw(ClientMsg::Typing { scope, typing: true });
        } else if !non_empty && self.state.last_sent_typing {
            let scope = self.state.active.clone();
            self.state.last_sent_typing = false;
            self.send_raw(ClientMsg::Typing { scope, typing: false });
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let dt = ctx.input(|i| i.stable_dt);
        self.frame_time = now;

        // Tema geçişi + global stil
        self.theme.update(dt);
        let theme = self.theme.current.clone();
        ctx.set_global_style(make_style(&theme));
        widgets::background(&ctx, &theme);

        // Sunucu olayları
        if let Some(handle) = &self.server {
            self.server_ui.drain(handle);
        }

        // Ağ olayları
        let mut events: Vec<crate::net::NetEvent> = Vec::new();
        if let Some(client) = &self.client {
            while let Ok(ev) = client.events.try_recv() {
                events.push(ev);
            }
        }
        let mut out: Vec<ClientMsg> = Vec::new();
        let mut dead = false;
        for ev in events {
            if matches!(ev, crate::net::NetEvent::Disconnected { .. }) {
                dead = true;
            }
            apply_event(&mut self.state, ev, now, &mut out);
        }
        if dead {
            self.client = None;
        }
        if let Some(client) = &self.client {
            for m in out {
                client.send(m);
            }
        }

        if self.state.just_connected {
            self.focus_input = true;
            self.state.just_connected = false;
        }
        self.state.drop_expired_typing();
        self.typing_tick();

        // ---- Üst bar ----
        egui::Panel::top("topbar")
            .frame(
                egui::Frame::default()
                    .fill(theme.panel)
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_rgba_unmultiplied(
                            theme.panel_stroke.r(),
                            theme.panel_stroke.g(),
                            theme.panel_stroke.b(),
                            110,
                        ),
                    ))
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ui, |ui| topbar(ui, self));

        // ---- İçerik ----
        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ui, |ui| {
                match self.tab {
                    Tab::Server => match serverpanel::ui(ui, &mut self.server_ui, &theme) {
                        ServerAction::Start => {
                            let port = self
                                .server_ui
                                .port
                                .trim()
                                .parse::<u16>()
                                .unwrap_or(5000);
                            match server::start(port) {
                                Ok(handle) => {
                                    self.server_ui.drain(&handle);
                                    self.server = Some(handle);
                                }
                                Err(e) => {
                                    let ts = chrono::Local::now().format("%H:%M:%S").to_string();
                                    self.server_ui.logs.push((
                                        crate::protocol::LogLevel::Error,
                                        format!("Başlatılamadı: {e}"),
                                        ts,
                                    ));
                                }
                            }
                        }
                        ServerAction::Stop => {
                            if let Some(mut h) = self.server.take() {
                                h.stop();
                                self.server_ui.drain(&h);
                            }
                            self.server_ui.running = false;
                            self.server_ui.port_actual = None;
                        }
                        ServerAction::None => {}
                    },
                    Tab::Client => {
                        if self.state.conn == ConnState::Connected {
                            let actions = chat::ui(
                                ui,
                                &mut self.state,
                                &self.bridge,
                                &theme,
                                &mut self.emoji_open,
                                &mut self.analysis,
                                &mut self.started_typing_at,
                                &mut self.focus_input,
                            );
                            for a in actions {
                                match a {
                                    chat::Action::Send(text) => self.send_text(&text),
                                    chat::Action::SwitchScope(s) => self.switch_scope(s),
                                    chat::Action::StartDm(u) => self.start_dm(u),
                                    chat::Action::None => {}
                                }
                            }
                        } else {
                            match connect::ui(ui, &mut self.state, &self.bridge, &theme) {
                                connect::ConnectAction::Connect => self.start_connect(),
                                connect::ConnectAction::None => {}
                            }
                        }
                    }
                }
            });

        ctx.request_repaint();
    }
}

fn topbar(ui: &mut egui::Ui, app: &mut App) {
    let theme = app.theme.current.clone();
    let now = ui.input(|i| i.time);

    ui.horizontal(|ui| {
        ui.set_height(30.0);

        // Logo
        let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(170.0, 26.0), Sense::hover());
        let p = ui.painter().clone();
        widgets::glow_text(
            &p,
            Pos2::new(logo_rect.left(), logo_rect.center().y),
            "🦀 RustChat",
            FontId::new(19.0, FontFamily::Proportional),
            theme.text,
            theme.accent,
        );

        ui.add_space(18.0);

        // Sekmeler
        tab(ui, app, Tab::Server, "Sunucu", &theme, now);
        ui.add_space(8.0);
        tab(ui, app, Tab::Client, "İstemci", &theme, now);

        ui.add_space(20.0);

        // Java rozeti
        let (jlabel, jcolor) = match app.bridge.status() {
            JavaStatus::Active => ("Java", theme.ok),
            JavaStatus::Starting => ("Java…", theme.accent2),
            JavaStatus::Failed(_) => ("Java kapalı", theme.dim),
            JavaStatus::Disabled => ("Java yok", theme.dim),
        };
        let jr = ui.allocate_exact_size(Vec2::new(86.0, 22.0), Sense::hover()).0;
        let jp = ui.painter().clone();
        let [r, g, b, _] = jcolor.to_array();
        jp.rect_filled(
            jr,
            CornerRadius::same(99),
            Color32::from_rgba_unmultiplied(r, g, b, 22),
        );
        jp.circle_filled(Pos2::new(jr.left() + 10.0, jr.center().y), 3.5, jcolor);
        jp.text(
            Pos2::new(jr.left() + 20.0, jr.center().y),
            egui::Align2::LEFT_CENTER,
            format!("☕ {jlabel}"),
            FontId::new(11.0, FontFamily::Proportional),
            jcolor,
        );

        ui.add_space(20.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Tema noktaları
            for (i, t) in themes().iter().enumerate() {
                let selected = app.theme.target_index() == i;
                let size = if selected { 26.0 } else { 20.0 };
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(size, size), Sense::click());
                let p = ui.painter().clone();
                let c = rect.center();
                if selected {
                    let ring = ui
                        .ctx()
                        .animate_bool(egui::Id::new(("theme_ring", i)), true);
                    p.circle_stroke(
                        c,
                        size * 0.5 + 2.0 + ring * 2.0,
                        Stroke::new(1.6, t.accent),
                    );
                }
                p.circle_filled(c, size * 0.5 - 3.0, t.accent);
                p.circle_filled(c, size * 0.5 - 6.0, t.bg2);
                if resp.clicked() {
                    app.theme.select(i);
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    resp.on_hover_text(t.name);
                }
                ui.add_space(5.0);
            }
        });
    });
}

fn tab(ui: &mut egui::Ui, app: &mut App, target: Tab, label: &str, theme: &crate::theme::Theme, now: f64) {
    let selected = app.tab == target;
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(15.0, FontFamily::Proportional),
        theme.text,
    );
    let size = galley.size() + Vec2::new(26.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter().clone();

    let anim = ui
        .ctx()
        .animate_bool(egui::Id::new(("tab_sel", label)), selected);
    let hover = ui
        .ctx()
        .animate_bool(egui::Id::new(("tab_h", label)), resp.hovered());

    if hover > 0.01 {
        p.rect_filled(
            rect,
            CornerRadius::same(10),
            Color32::from_rgba_unmultiplied(255, 255, 255, (8.0 * hover) as u8),
        );
    }
    if anim > 0.01 {
        let [r, g, b, _] = theme.accent.to_array();
        p.rect_filled(
            rect,
            CornerRadius::same(10),
            Color32::from_rgba_unmultiplied(r, g, b, (22.0 * anim) as u8),
        );
        let uw = (rect.width() - 16.0) * anim;
        widgets::gradient_rect(
            &p,
            Rect::from_min_size(
                Pos2::new(rect.center().x - uw * 0.5, rect.bottom() - 3.0),
                Vec2::new(uw, 2.5),
            ),
            CornerRadius::same(2),
            theme.accent,
            theme.accent2,
        );
    }

    let text_color = if selected {
        theme.text
    } else {
        widgets::lerp(theme.dim, theme.text, hover * 0.8)
    };
    p.galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );

    if resp.clicked() {
        app.tab = target;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let _ = now;
}

fn make_style(theme: &crate::theme::Theme) -> egui::Style {
    let mut style = egui::Style::default();
    let v = &mut style.visuals;
    v.panel_fill = theme.panel;
    v.window_fill = theme.panel;
    v.extreme_bg_color = widgets::lerp(theme.bg, Color32::BLACK, 0.35);
    v.faint_bg_color = widgets::lerp(theme.bg, Color32::BLACK, 0.18);
    v.override_text_color = Some(theme.text);
    v.selection.bg_fill = theme.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.noninteractive.bg_fill = widgets::lerp(theme.panel, theme.bg, 0.5);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme.text);
    v.widgets.inactive.bg_fill = widgets::lerp(theme.panel, theme.accent, 0.10);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, theme.text);
    v.widgets.inactive.weak_bg_fill = widgets::lerp(theme.panel, theme.accent, 0.14);
    v.widgets.hovered.bg_fill = widgets::lerp(theme.panel, theme.accent, 0.22);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, theme.accent2);
    v.widgets.hovered.weak_bg_fill = widgets::lerp(theme.panel, theme.accent, 0.24);
    v.widgets.active.bg_fill = widgets::lerp(theme.panel, theme.accent, 0.34);
    v.widgets.active.fg_stroke = Stroke::new(1.5, theme.accent2);
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style
}

pub fn run() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustChat")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "RustChat",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    );
    if let Err(e) = result {
        eprintln!("Pencere başlatılamadı: {e}");
        std::process::exit(1);
    }
}
