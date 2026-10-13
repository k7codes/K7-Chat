//! Uygulama durumu: bağlantı, konuşmalar, okundu bilgileri.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::{
    net::NetEvent,
    protocol::{ChatMsg, ClientMsg, MsgId, Scope, ServerMsg, clean_room},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Server,
    Client,
}

pub struct ShownMsg {
    pub msg: ChatMsg,
    pub shown_at: f64,
}

pub struct ClientUi {
    pub conn: ConnState,
    pub disconnect_reason: Option<String>,
    pub server_ip: String,
    pub server_port: String,
    pub username: String,
    pub self_name: String,
    pub users: Vec<String>,
    pub rooms: Vec<String>,
    pub active: Scope,
    pub convs: HashMap<String, Vec<ShownMsg>>,
    pub unread: HashMap<String, u32>,
    pub reads: HashMap<String, HashMap<String, MsgId>>,
    pub typing: HashMap<String, HashMap<String, Instant>>,
    pub input: String,
    pub last_sent_typing: bool,
    pub last_typing_sent: Instant,
    pub just_connected: bool,
    pub connect_started: Option<Instant>,
    pub error_flash: Option<(String, Instant)>,
}

impl Default for ClientUi {
    fn default() -> Self {
        Self {
            conn: ConnState::Disconnected,
            disconnect_reason: None,
            server_ip: "127.0.0.1".into(),
            server_port: "5000".into(),
            username: String::new(),
            self_name: String::new(),
            users: Vec::new(),
            rooms: crate::protocol::DEFAULT_ROOMS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            active: Scope::Room {
                room: crate::protocol::DEFAULT_ROOMS[0].to_string(),
            },
            convs: HashMap::new(),
            unread: HashMap::new(),
            reads: HashMap::new(),
            typing: HashMap::new(),
            input: String::new(),
            last_sent_typing: false,
            last_typing_sent: Instant::now(),
            just_connected: false,
            connect_started: None,
            error_flash: None,
        }
    }
}

impl ClientUi {
    pub fn conv_mut(&mut self, scope: &Scope) -> &mut Vec<ShownMsg> {
        self.convs.entry(scope.key()).or_default()
    }

    pub fn conv(&self, scope: &Scope) -> &[ShownMsg] {
        self.convs
            .get(&scope.key())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn activate(&mut self, scope: Scope, now: f64) {
        self.active = scope.clone();
        self.unread.remove(&scope.key());
        // Odaya girişte son mesajlar sırayla (stagger) belirsin
        if let Some(v) = self.convs.get_mut(&scope.key()) {
            let n = v.len();
            for (i, sm) in v.iter_mut().enumerate() {
                let from_end = n - 1 - i;
                sm.shown_at = if from_end < 12 {
                    now + (11 - from_end) as f64 * 0.03
                } else {
                    now - 1.0
                };
            }
        }
    }

    pub fn set_error(&mut self, text: String) {
        self.error_flash = Some((text, Instant::now()));
    }

    /// Sunucudan gelen mesajı duruma işlet.
    pub fn apply(&mut self, msg: ServerMsg, now: f64) -> Option<Outgoing> {
        match msg {
            ServerMsg::Welcome {
                name, users, rooms, ..
            } => {
                self.self_name = name;
                self.users = users;
                self.rooms = rooms;
                self.conn = ConnState::Connected;
                self.just_connected = true;
                self.disconnect_reason = None;
                None
            }
            ServerMsg::History { scope, msgs } => {
                let key = scope.key();
                let entry = self.convs.entry(key.clone()).or_default();
                for m in msgs {
                    if !entry.iter().any(|s| s.msg.id == m.id) {
                        entry.push(ShownMsg { msg: m, shown_at: now - 1.0 });
                    }
                }
                self.unread.remove(&key);
                None
            }
            ServerMsg::Users { users } => {
                self.users = users;
                None
            }
            ServerMsg::Rooms { rooms } => {
                self.rooms = rooms;
                None
            }
            ServerMsg::Chat(m) => {
                let key = m.scope.key();
                let is_own = m.from == self.self_name;
                let is_active = key == self.active.key();
                let id = m.id;
                let system = m.system;
                let scope = m.scope.clone();
                {
                    let entry = self.convs.entry(key.clone()).or_default();
                    if !entry.iter().any(|s| s.msg.id == id) {
                        entry.push(ShownMsg { msg: m, shown_at: now });
                    }
                }
                if !is_own && !is_active {
                    *self.unread.entry(key).or_insert(0) += 1;
                }
                if !system && (is_own || is_active) {
                    return Some(Outgoing::AutoRead {
                        scope,
                        up_to: id,
                    });
                }
                None
            }
            ServerMsg::UserJoined { name } => {
                if !self.users.contains(&name) {
                    self.users.push(name);
                    self.users.sort();
                }
                None
            }
            ServerMsg::UserLeft { name } => {
                self.users.retain(|u| u != &name);
                None
            }
            ServerMsg::Typing { scope, user, typing } => {
                if user == self.self_name {
                    return None;
                }
                let key = scope.key();
                let entry = self.typing.entry(key).or_default();
                if typing {
                    entry.insert(user, Instant::now());
                } else {
                    entry.remove(&user);
                }
                None
            }
            ServerMsg::ReadBy { scope, user, up_to } => {
                let key = scope.key();
                self.reads.entry(key).or_default().insert(user, up_to);
                None
            }
            ServerMsg::Error { text } => {
                self.set_error(text);
                None
            }
            ServerMsg::Pong => None,
        }
    }

    /// Verilen mesaj okundu mu? (✓✓)
    pub fn is_read(&self, scope: &Scope, id: MsgId, own: bool) -> bool {
        if !own {
            return false;
        }
        if let Some(readers) = self.reads.get(&scope.key()) {
            return readers.values().any(|&v| v >= id);
        }
        false
    }

    pub fn drop_expired_typing(&mut self) {
        let now = Instant::now();
        for map in self.typing.values_mut() {
            map.retain(|_, at| now.duration_since(*at) < Duration::from_secs(4));
        }
        self.typing.retain(|_, m| !m.is_empty());
    }

    pub fn typing_display(&self, scope: &Scope) -> Vec<String> {
        self.typing
            .get(&scope.key())
            .map(|m| {
                let mut v: Vec<String> = m.keys().cloned().collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }
}

/// UI'dan ağ katmanına giden hazır mesajlar.
pub enum Outgoing {
    Send(ClientMsg),
    AutoRead { scope: Scope, up_to: MsgId },
}

/// Ağ olayını UI durumuna ve giden mesajlara çevir.
pub fn apply_event(
    state: &mut ClientUi,
    ev: NetEvent,
    now: f64,
    out: &mut Vec<ClientMsg>,
) {
    match ev {
        NetEvent::Connected => {}
        NetEvent::Msg(m) => {
            if let Some(o) = state.apply(m, now) {
                match o {
                    Outgoing::Send(c) => out.push(c),
                    Outgoing::AutoRead { scope, up_to } => {
                        out.push(ClientMsg::Read { scope, up_to })
                    }
                }
            }
        }
        NetEvent::Disconnected { reason } => {
            state.conn = ConnState::Disconnected;
            if reason != "Kapatıldı" {
                state.disconnect_reason = Some(reason);
            }
        }
    }
}

/// Kullanıcının girdiği metni Java/Rust hattından geçirip gönderilecek mesaja çevir.
pub fn build_outgoing(
    state: &mut ClientUi,
    parsed: &serde_json::Value,
    now: f64,
) -> Vec<ClientMsg> {
    let kind = parsed
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("message");
    let mut out = Vec::new();

    match kind {
        "command" => {
            let cmd = parsed.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
            let rest = parsed.get("rest").and_then(|v| v.as_str()).unwrap_or("");
            match cmd {
                "join" | "j" => {
                    let room = clean_room(rest.split_whitespace().next().unwrap_or(""));
                    if !room.is_empty() {
                        out.push(ClientMsg::JoinRoom {
                            room: room.clone(),
                        });
                        state.activate(Scope::Room { room }, now);
                    } else {
                        state.set_error("Kullanım: /join <oda>".into());
                    }
                }
                "msg" | "m" => {
                    let mut it = rest.split_whitespace();
                    let peer = it.next().unwrap_or("").to_string();
                    let body: Vec<&str> = it.collect();
                    if peer.is_empty() || body.is_empty() {
                        state.set_error("Kullanım: /msg <kullanıcı> <mesaj>".into());
                    } else {
                        let scope = Scope::Dm { peer: peer.clone() };
                        out.push(ClientMsg::OpenDm { peer });
                        out.push(ClientMsg::Say {
                            scope: scope.clone(),
                            text: body.join(" "),
                            action: false,
                        });
                        state.activate(scope, now);
                    }
                }
                "me" => {
                    let body = rest.to_string();
                    if body.is_empty() {
                        state.set_error("Kullanım: /me <eylem>".into());
                    } else {
                        out.push(ClientMsg::Say {
                            scope: state.active.clone(),
                            text: body,
                            action: true,
                        });
                    }
                }
                "nick" => {
                    let new = crate::protocol::clean_name(rest);
                    if new.is_empty() {
                        state.set_error("Kullanım: /nick <yeni_isim>".into());
                    } else {
                        out.push(ClientMsg::Rename { name: new });
                    }
                }
                "clear" => {
                    let key = state.active.key();
                    if let Some(v) = state.convs.get_mut(&key) {
                        v.clear();
                    }
                }
                "help" | "h" => {
                    state.set_error(
                        "/join, /msg, /me, /nick, /clear, /help, /quit".into(),
                    );
                }
                "quit" | "q" | "exit" => {
                    // UI tarafı conn==Disconnected görünce istemciyi düşürür
                    state.conn = ConnState::Disconnected;
                }
                _ => state.set_error(format!("Bilinmeyen komut: /{cmd}")),
            }
        }
        _ => {
            let text = parsed
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = crate::protocol::clean_text(&text);
            if !text.is_empty() {
                out.push(ClientMsg::Say {
                    scope: state.active.clone(),
                    text,
                    action: false,
                });
            }
        }
    }
    out
}
