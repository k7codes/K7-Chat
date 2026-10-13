use serde::{Deserialize, Serialize};

pub type MsgId = u64;
pub type UserId = u64;

pub const DEFAULT_ROOMS: &[&str] = &["genel", "rust", "rasgele"];

/// Bir mesajın kime ait olduğu: oda ya da birebir özel mesaj.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum Scope {
    Room { room: String },
    Dm { peer: String },
}

impl Scope {
    /// UI'da harita anahtarı: "#oda" veya "@kisi".
    pub fn key(&self) -> String {
        match self {
            Scope::Room { room } => format!("#{}", room.to_lowercase()),
            Scope::Dm { peer } => format!("@{}", peer.to_lowercase()),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Scope::Room { room } => format!("#{room}"),
            Scope::Dm { peer } => format!("@{peer}"),
        }
    }
}

/// Özel mesajların çift anahtarı (sıralı) — sunucu geçmişinde kullanılır.
pub fn dm_pair_key(a: &str, b: &str) -> String {
    let (x, y) = if a.to_lowercase() < b.to_lowercase() {
        (a, b)
    } else {
        (b, a)
    };
    format!("{x}\u{1}{y}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMsg {
    pub id: MsgId,
    pub from: String,
    pub ts: i64,
    pub text: String,
    pub scope: Scope,
    #[serde(default)]
    pub action: bool,
    #[serde(default)]
    pub system: bool,
    /// Yalnızca DM teslimatında kullanılır: alıcının görmesi gereken taraf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dm_to: Option<String>,
}

impl ChatMsg {
    pub fn time_string(&self) -> String {
        let dt = chrono::DateTime::from_timestamp(self.ts, 0)
            .unwrap_or_default();
        dt.format("%H:%M").to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum ClientMsg {
    Hello { name: String },
    Say { scope: Scope, text: String, action: bool },
    JoinRoom { room: String },
    OpenDm { peer: String },
    Typing { scope: Scope, typing: bool },
    Read { scope: Scope, up_to: MsgId },
    Rename { name: String },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum ServerMsg {
    Welcome {
        self_id: UserId,
        name: String,
        users: Vec<String>,
        rooms: Vec<String>,
    },
    History {
        scope: Scope,
        msgs: Vec<ChatMsg>,
    },
    Users {
        users: Vec<String>,
    },
    Rooms {
        rooms: Vec<String>,
    },
    Chat(ChatMsg),
    UserJoined {
        name: String,
    },
    UserLeft {
        name: String,
    },
    Typing {
        scope: Scope,
        user: String,
        typing: bool,
    },
    ReadBy {
        scope: Scope,
        user: String,
        up_to: MsgId,
    },
    Error {
        text: String,
    },
    Pong,
}

/// Sunucu olayları (arayüz günlüğü için).
#[derive(Debug, Clone)]
pub enum ServerEvent {
    Started { port: u16 },
    Stopped,
    Log { level: LogLevel, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

// ---------- framing: 4 byte uzunluk (big endian) + JSON ----------

pub const MAX_FRAME: u32 = 1 << 20;

pub fn encode_frame<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = serde_json::to_vec(msg).expect("serialize");
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

/// Eşzamanlı (test/selftest) çerçeve okuma.
pub fn read_frame_sync<R: std::io::Read, T: for<'de> Deserialize<'de>>(
    r: &mut R,
) -> std::io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub fn write_frame_sync<W: std::io::Write, T: Serialize>(
    w: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    w.write_all(&encode_frame(msg))?;
    w.flush()
}

pub async fn read_frame_async<R: tokio::io::AsyncRead + Unpin, T: for<'de> Deserialize<'de>>(
    r: &mut R,
) -> std::io::Result<T> {
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub async fn write_frame_async<W: tokio::io::AsyncWrite + Unpin, T: Serialize>(
    w: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    w.write_all(&encode_frame(msg)).await?;
    w.flush().await
}

/// Kullanıcı adı / oda adı doğrulama ve temizleme.
pub fn clean_name(raw: &str) -> String {
    let s: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(24)
        .collect();
    s.trim().to_string()
}

pub fn clean_room(raw: &str) -> String {
    let s: String = raw
        .trim()
        .trim_start_matches('#')
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .take(24)
        .collect();
    s.trim().to_string()
}

pub fn clean_text(raw: &str) -> String {
    raw.replace(['\r', '\0'], "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let msg = ClientMsg::Say {
            scope: Scope::Room {
                room: "genel".into(),
            },
            text: "merhaba dünya 🔥".into(),
            action: false,
        };
        let bytes = encode_frame(&msg);
        let mut cursor = std::io::Cursor::new(bytes);
        let back: ClientMsg = read_frame_sync(&mut cursor).unwrap();
        match back {
            ClientMsg::Say { text, scope, .. } => {
                assert_eq!(text, "merhaba dünya 🔥");
                assert_eq!(scope, Scope::Room { room: "genel".into() });
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn scope_keys() {
        assert_eq!(Scope::Room { room: "Genel".into() }.key(), "#genel");
        assert_eq!(Scope::Dm { peer: "Ali".into() }.key(), "@ali");
        assert_eq!(dm_pair_key("ali", "veli"), dm_pair_key("veli", "ali"));
    }

    #[test]
    fn name_cleaning() {
        assert_eq!(clean_name("  ayşe  "), "ayşe");
        assert_eq!(clean_room("#Rust_2"), "Rust_2");
        assert_eq!(clean_text("a\r\nb\0"), "a\nb");
    }
}
