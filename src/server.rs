use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel},
    },
};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{UnboundedSender, unbounded_channel},
};

use crate::protocol::{
    self, ChatMsg, ClientMsg, LogLevel, MsgId, Scope, ServerEvent, ServerMsg, UserId,
    dm_pair_key,
};

const ROOM_HISTORY_CAP: usize = 500;
const DM_HISTORY_CAP: usize = 200;

pub struct ServerHandle {
    pub events: Receiver<ServerEvent>,
    pub port: u16,
    stop: Option<UnboundedSender<()>>,
    log_tx: Sender<ServerEvent>,
    aborts: Vec<tokio::task::AbortHandle>,
}

impl ServerHandle {
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
            for a in self.aborts.drain(..) {
                a.abort();
            }
            let _ = self.log_tx.send(ServerEvent::Log {
                level: LogLevel::Info,
                text: "Sunucu durduruldu.".into(),
            });
            let _ = self.log_tx.send(ServerEvent::Stopped);
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

struct UserEntry {
    name: String,
    tx: UnboundedSender<ServerMsg>,
}

struct Room {
    members: HashSet<UserId>,
    history: Vec<ChatMsg>,
}

struct Hub {
    users: HashMap<UserId, UserEntry>,
    by_name: HashMap<String, UserId>,
    rooms: HashMap<String, Room>,
    dms: HashMap<String, Vec<ChatMsg>>,
    next_user: UserId,
    next_msg: MsgId,
    log_tx: Sender<ServerEvent>,
}

impl Hub {
    fn new(log_tx: Sender<ServerEvent>) -> Self {
        let mut rooms = HashMap::new();
        for r in protocol::DEFAULT_ROOMS {
            rooms.insert(
                r.to_string(),
                Room {
                    members: HashSet::new(),
                    history: Vec::new(),
                },
            );
        }
        Self {
            users: HashMap::new(),
            by_name: HashMap::new(),
            rooms,
            dms: HashMap::new(),
            next_user: 1,
            next_msg: 1,
            log_tx,
        }
    }

    fn log(&self, level: LogLevel, text: impl Into<String>) {
        let _ = self.log_tx.send(ServerEvent::Log {
            level,
            text: text.into(),
        });
    }

    fn unique_name(&self, base: &str) -> String {
        let mut name = base.to_string();
        let mut n = 2;
        while self.by_name.contains_key(&name.to_lowercase()) {
            name = format!("{base}_{n}");
            n += 1;
        }
        name
    }

    fn send_to(&self, id: UserId, msg: ServerMsg) {
        if let Some(u) = self.users.get(&id) {
            let _ = u.tx.send(msg);
        }
    }

    fn name_of(&self, id: UserId) -> String {
        self.users.get(&id).map(|u| u.name.clone()).unwrap_or_default()
    }

    fn broadcast_room(&self, room: &str, except: Option<UserId>, msg: &ServerMsg) {
        if let Some(r) = self.rooms.get(room) {
            for &uid in &r.members {
                if Some(uid) != except {
                    self.send_to(uid, msg.clone());
                }
            }
        }
    }

    fn broadcast_all(&self, except: Option<UserId>, msg: ServerMsg) {
        for &uid in self.users.keys() {
            if Some(uid) != except {
                self.send_to(uid, msg.clone());
            }
        }
    }

    fn push_system(&mut self, room: &str, text: String) {
        let msg = ChatMsg {
            id: self.next_msg(),
            from: "sistem".into(),
            ts: now_ts(),
            text,
            scope: Scope::Room { room: room.into() },
            action: false,
            system: true,
            dm_to: None,
        };
        if let Some(r) = self.rooms.get_mut(room) {
            r.history.push(msg.clone());
            if r.history.len() > ROOM_HISTORY_CAP {
                let cut = r.history.len() - ROOM_HISTORY_CAP;
                r.history.drain(..cut);
            }
        }
        self.broadcast_room(room, None, &ServerMsg::Chat(msg));
    }

    fn next_msg(&mut self) -> MsgId {
        let id = self.next_msg;
        self.next_msg += 1;
        id
    }

    fn user_list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.users.values().map(|u| u.name.clone()).collect();
        v.sort();
        v
    }

    fn room_list(&self) -> Vec<String> {
        let mut v: Vec<String> = self.rooms.keys().cloned().collect();
        v.sort();
        v
    }
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

type SharedHub = Arc<Mutex<Hub>>;

/// Sunucuyu başlat; UI olayları `events` kanalından okur.
pub fn start(port: u16) -> std::io::Result<ServerHandle> {
    let (log_tx, log_rx) = channel::<ServerEvent>();
    let (stop_tx, mut stop_rx) = unbounded_channel::<()>();

    let rt = crate::net::runtime();
    let listener = rt.block_on(async { TcpListener::bind(("0.0.0.0", port)).await })?;
    let actual_port = listener.local_addr()?.port();

    let hub: SharedHub = Arc::new(Mutex::new(Hub::new(log_tx.clone())));
    let _ = log_tx.send(ServerEvent::Started { port: actual_port });
    let _ = log_tx.send(ServerEvent::Log {
        level: LogLevel::Info,
        text: format!("Sunucu dinlemeye başladı: 0.0.0.0:{actual_port}"),
    });

    let mut aborts = Vec::new();

    // Kabul döngüsü
    {
        let hub = hub.clone();
        let log_tx = log_tx.clone();
        let handle = rt.spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, addr)) => {
                                let hub = hub.clone();
                                let log_tx = log_tx.clone();
                                tokio::spawn(async move {
                                    let _ = log_tx.send(ServerEvent::Log {
                                        level: LogLevel::Info,
                                        text: format!("Yeni bağlantı: {addr}"),
                                    });
                                    if let Err(e) = handle_conn(stream, hub.clone()).await {
                                        let _ = log_tx.send(ServerEvent::Log {
                                            level: LogLevel::Warn,
                                            text: format!("Bağlantı kapandı ({addr}): {e}"),
                                        });
                                    }
                                });
                            }
                            Err(e) => {
                                let _ = log_tx.send(ServerEvent::Log {
                                    level: LogLevel::Error,
                                    text: format!("Accept hatası: {e}"),
                                });
                                break;
                            }
                        }
                    }
                }
            }
        });
        aborts.push(handle.abort_handle());
    }

    Ok(ServerHandle {
        events: log_rx,
        port: actual_port,
        stop: Some(stop_tx),
        log_tx,
        aborts,
    })
}

async fn handle_conn(mut stream: TcpStream, hub: SharedHub) -> Result<(), String> {
    stream.set_nodelay(true).ok();

    // İlk mesaj Hello olmalı
    let first: ClientMsg = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        protocol::read_frame_async(&mut stream),
    )
    .await
    .map_err(|_| "hello zaman aşımı".to_string())?
    .map_err(|e| e.to_string())?;

    let ClientMsg::Hello { name } = first else {
        return Err("ilk mesaj Hello olmalı".into());
    };

    let name = protocol::clean_name(&name);
    if name.is_empty() {
        let mut s = stream;
        let _ = protocol::write_frame_async(
            &mut s,
            &ServerMsg::Error {
                text: "Geçersiz kullanıcı adı".into(),
            },
        )
        .await;
        return Err("geçersiz isim".into());
    }

    let (tx, mut rx) = unbounded_channel::<ServerMsg>();
    let (mut read_half, mut write_half) = tokio::io::split(stream);

    let user_id: UserId;
    {
        let mut h = hub.lock().unwrap();
        let uname = h.unique_name(&name);
        user_id = h.next_user;
        h.next_user += 1;
        h.users.insert(
            user_id,
            UserEntry {
                name: uname.clone(),
                tx,
            },
        );
        h.by_name.insert(uname.to_lowercase(), user_id);
        h.log(
            LogLevel::Info,
            format!("{uname} bağlandı (id={user_id}, {} kullanıcı)", h.users.len()),
        );

        let users = h.user_list();
        let rooms = h.room_list();
        h.send_to(
            user_id,
            ServerMsg::Welcome {
                self_id: user_id,
                name: uname.clone(),
                users,
                rooms,
            },
        );

        // Genel odaya ekle + geçmiş gönder
        let default_room = protocol::DEFAULT_ROOMS[0].to_string();
        if let Some(r) = h.rooms.get_mut(&default_room) {
            r.members.insert(user_id);
        }
        let hist: Vec<ChatMsg> = h
            .rooms
            .get(&default_room)
            .map(|r| r.history.clone())
            .unwrap_or_default();
        h.send_to(
            user_id,
            ServerMsg::History {
                scope: Scope::Room {
                    room: default_room.clone(),
                },
                msgs: hist,
            },
        );

        let joined_msg = ServerMsg::UserJoined { name: uname.clone() };
        h.broadcast_all(Some(user_id), joined_msg);
        let system_text = format!("{uname} sohbete katıldı.");
        drop(h);

        let mut h = hub.lock().unwrap();
        h.push_system(&default_room, system_text);
    }

    // Yazıcı: rx → soket
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if protocol::write_frame_async(&mut write_half, &msg).await.is_err() {
                break;
            }
        }
    });

    // Okuyucu: soket → hub
    let result: Result<(), String> = async {
        loop {
            let msg: ClientMsg =
                match protocol::read_frame_async(&mut read_half).await {
                    Ok(m) => m,
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(e.to_string()),
                };
            dispatch(&hub, user_id, msg).await;
        }
        Ok(())
    }
    .await;

    // Ayrılma temizliği
    {
        let mut h = hub.lock().unwrap();
        if let Some(u) = h.users.remove(&user_id) {
            h.by_name.remove(&u.name.to_lowercase());
            let rooms: Vec<String> = h
                .rooms
                .iter()
                .filter(|(_, r)| r.members.contains(&user_id))
                .map(|(k, _)| k.clone())
                .collect();
            for r in &rooms {
                if let Some(room) = h.rooms.get_mut(r) {
                    room.members.remove(&user_id);
                }
                let text = format!("{} sohbetten ayrıldı.", u.name);
                h.push_system(r, text);
            }
            let names = h.user_list();
            let left = ServerMsg::UserLeft { name: u.name.clone() };
            h.broadcast_all(None, left);
            h.broadcast_all(None, ServerMsg::Users { users: names });
            h.log(
                LogLevel::Info,
                format!("{} ayrıldı ({} kullanıcı kaldı)", u.name, h.users.len()),
            );
        }
    }
    writer.abort();
    result
}

async fn dispatch(hub: &SharedHub, uid: UserId, msg: ClientMsg) {
    match msg {
        ClientMsg::Ping => {
            let h = hub.lock().unwrap();
            h.send_to(uid, ServerMsg::Pong);
        }
        ClientMsg::Say {
            scope,
            text,
            action,
        } => {
            let text = protocol::clean_text(&text);
            if text.is_empty() {
                return;
            }
            let mut h = hub.lock().unwrap();
            let from = h.name_of(uid);
            if from.is_empty() {
                return;
            }
            match scope {
                Scope::Room { room } => {
                    let room = protocol::clean_room(&room);
                    if room.is_empty() {
                        return;
                    }
                    if !h.rooms.contains_key(&room) {
                        h.rooms.insert(
                            room.clone(),
                            Room {
                                members: HashSet::new(),
                                history: Vec::new(),
                            },
                        );
                        h.broadcast_all(None, ServerMsg::Rooms { rooms: h.room_list() });
                    }
                    let is_member = h.rooms.get(&room).is_some_and(|r| r.members.contains(&uid));
                    if !is_member {
                        if let Some(r) = h.rooms.get_mut(&room) {
                            r.members.insert(uid);
                        }
                    }
                    let chat = ChatMsg {
                        id: h.next_msg(),
                        from: from.clone(),
                        ts: now_ts(),
                        text,
                        scope: Scope::Room { room: room.clone() },
                        action,
                        system: false,
                        dm_to: None,
                    };
                    if let Some(r) = h.rooms.get_mut(&room) {
                        r.history.push(chat.clone());
                        if r.history.len() > ROOM_HISTORY_CAP {
                            let cut = r.history.len() - ROOM_HISTORY_CAP;
                            r.history.drain(..cut);
                        }
                    }
                    let members: Vec<UserId> = h
                        .rooms
                        .get(&room)
                        .map(|r| r.members.iter().copied().collect())
                        .unwrap_or_default();
                    for m in members {
                        h.send_to(m, ServerMsg::Chat(chat.clone()));
                    }
                }
                Scope::Dm { peer } => {
                    let peer = protocol::clean_name(&peer);
                    if peer.is_empty() {
                        return;
                    }
                    let peer_id = h
                        .by_name
                        .get(&peer.to_lowercase())
                        .copied()
                        .or_else(|| h.by_name.values().find(|&&id| h.name_of(id).eq_ignore_ascii_case(&peer)).copied());
                    let Some(peer_id) = peer_id else {
                        h.send_to(
                            uid,
                            ServerMsg::Error {
                                text: format!("Böyle bir kullanıcı yok: {peer}"),
                            },
                        );
                        return;
                    };
                    let peer_name = h.name_of(peer_id);
                    let key = dm_pair_key(&from, &peer_name);
                    let chat_for_peer = ChatMsg {
                        id: h.next_msg(),
                        from: from.clone(),
                        ts: now_ts(),
                        text: text.clone(),
                        scope: Scope::Dm { peer: from.clone() },
                        action,
                        system: false,
                        dm_to: Some(peer_name.clone()),
                    };
                    let chat_for_me = ChatMsg {
                        scope: Scope::Dm { peer: peer_name.clone() },
                        dm_to: Some(from.clone()),
                        ..chat_for_peer.clone()
                    };
                    {
                        let entry = h.dms.entry(key).or_default();
                        entry.push(chat_for_peer.clone());
                        if entry.len() > DM_HISTORY_CAP {
                            let cut = entry.len() - DM_HISTORY_CAP;
                            entry.drain(..cut);
                        }
                    }
                    h.send_to(peer_id, ServerMsg::Chat(chat_for_peer));
                    h.send_to(uid, ServerMsg::Chat(chat_for_me));
                }
            }
        }
        ClientMsg::JoinRoom { room } => {
            let room = protocol::clean_room(&room);
            if room.is_empty() {
                return;
            }
            let mut h = hub.lock().unwrap();
            if !h.rooms.contains_key(&room) {
                h.rooms.insert(
                    room.clone(),
                    Room {
                        members: HashSet::new(),
                        history: Vec::new(),
                    },
                );
                h.broadcast_all(None, ServerMsg::Rooms { rooms: h.room_list() });
            }
            let first_time = h
                .rooms
                .get(&room)
                .is_some_and(|r| !r.members.contains(&uid));
            if let Some(r) = h.rooms.get_mut(&room) {
                r.members.insert(uid);
            }
            let hist: Vec<ChatMsg> = h
                .rooms
                .get(&room)
                .map(|r| r.history.clone())
                .unwrap_or_default();
            h.send_to(
                uid,
                ServerMsg::History {
                    scope: Scope::Room { room: room.clone() },
                    msgs: hist,
                },
            );
            if first_time {
                let name = h.name_of(uid);
                h.push_system(&room, format!("{name} odaya katıldı."));
            }
        }
        ClientMsg::OpenDm { peer } => {
            let peer = protocol::clean_name(&peer);
            if peer.is_empty() {
                return;
            }
            let h = hub.lock().unwrap();
            let me = h.name_of(uid);
            let key = dm_pair_key(&me, &peer);
            let msgs: Vec<ChatMsg> = h
                .dms
                .get(&key)
                .map(|v| {
                    v.iter()
                        .map(|m| ChatMsg {
                            scope: Scope::Dm { peer: peer.clone() },
                            ..m.clone()
                        })
                        .collect()
                })
                .unwrap_or_default();
            h.send_to(
                uid,
                ServerMsg::History {
                    scope: Scope::Dm { peer },
                    msgs,
                },
            );
        }
        ClientMsg::Typing { scope, typing } => {
            let h = hub.lock().unwrap();
            let user = h.name_of(uid);
            let out = ServerMsg::Typing {
                scope: scope.clone(),
                user,
                typing,
            };
            match scope {
                Scope::Room { room } => {
                    h.broadcast_room(&room, Some(uid), &out);
                }
                Scope::Dm { peer } => {
                    if let Some(&pid) = h.by_name.get(&peer.to_lowercase()) {
                        h.send_to(pid, out);
                    }
                }
            }
        }
        ClientMsg::Read { scope, up_to } => {
            let h = hub.lock().unwrap();
            let user = h.name_of(uid);
            let out = ServerMsg::ReadBy {
                scope: scope.clone(),
                user,
                up_to,
            };
            match scope {
                Scope::Room { room } => {
                    h.broadcast_room(&room, Some(uid), &out);
                }
                Scope::Dm { peer } => {
                    if let Some(&pid) = h.by_name.get(&peer.to_lowercase()) {
                        h.send_to(pid, out);
                    }
                }
            }
        }
        ClientMsg::Rename { name } => {
            let new = protocol::clean_name(&name);
            if new.is_empty() {
                return;
            }
            let mut h = hub.lock().unwrap();
            let old = h.name_of(uid);
            if old == new {
                return;
            }
            if h.by_name.contains_key(&new.to_lowercase()) {
                h.send_to(
                    uid,
                    ServerMsg::Error {
                        text: format!("İsim kullanımda: {new}"),
                    },
                );
                return;
            }
            if h.users.contains_key(&uid) {
                h.by_name.remove(&old.to_lowercase());
                if let Some(u) = h.users.get_mut(&uid) {
                    u.name = new.clone();
                }
                h.by_name.insert(new.to_lowercase(), uid);
            }
            let rooms: Vec<String> = h
                .rooms
                .iter()
                .filter(|(_, r)| r.members.contains(&uid))
                .map(|(k, _)| k.clone())
                .collect();
            for r in rooms {
                h.push_system(&r, format!("{old} artık {new}."));
            }
            h.broadcast_all(None, ServerMsg::Users { users: h.user_list() });
            h.send_to(uid, ServerMsg::Users { users: h.user_list() });
        }
        ClientMsg::Hello { .. } => {
            let h = hub.lock().unwrap();
            h.send_to(
                uid,
                ServerMsg::Error {
                    text: "Zaten bağlandın".into(),
                },
            );
        }
    }
}
