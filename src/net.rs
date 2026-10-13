use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::protocol::{self, ClientMsg, ServerMsg};

/// Uygulama genelinde tek tokio runtime (UI + sunucu + istemci).
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .thread_name("rustchat-net")
            .build()
            .expect("tokio runtime")
    })
}

#[derive(Debug)]
pub enum NetEvent {
    Connected,
    Msg(ServerMsg),
    Disconnected { reason: String },
}

pub struct Client {
    pub out: UnboundedSender<ClientMsg>,
    pub events: Receiver<NetEvent>,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl Client {
    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn send(&self, msg: ClientMsg) {
        let _ = self.out.send(msg);
    }
}

/// Adrese bağlan; olaylar `events` kanalından akar. Kanal kapanınca bağlantı bitmiştir.
pub fn connect(addr: String) -> Client {
    let (out_tx, mut out_rx) = unbounded_channel::<ClientMsg>();
    let (ev_tx, ev_rx) = channel::<NetEvent>();
    let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = connected.clone();

    runtime().spawn(async move {
        let fail = |reason: String, flag: Arc<std::sync::atomic::AtomicBool>, ev: &Sender<NetEvent>| {
            flag.store(false, std::sync::atomic::Ordering::Relaxed);
            let _ = ev.send(NetEvent::Disconnected { reason });
        };

        let stream = match tokio::time::timeout(Duration::from_secs(6), TcpStream::connect(&addr))
            .await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                fail(format!("Bağlantı reddedildi: {e}"), flag, &ev_tx);
                return;
            }
            Err(_) => {
                fail("Zaman aşımı (sunucuya ulaşılamadı)".into(), flag, &ev_tx);
                return;
            }
        };
        stream.set_nodelay(true).ok();

        let (mut read_half, mut write_half) = tokio::io::split(stream);

        // İlk Hello istemciden: out kanalındaki ilk mesajı bekleme; connect() çağıran
        // taraf önce send(Hello) çağırır. out_rx ilk mesajı Hello varsayarız:
        let hello_opt = out_rx.recv().await;
        let Some(hello) = hello_opt else {
            fail("Hello gönderilmedi".into(), flag, &ev_tx);
            return;
        };
        if protocol::write_frame_async(&mut write_half, &hello)
            .await
            .is_err()
        {
            fail("Hello gönderilemedi".into(), flag, &ev_tx);
            return;
        }

        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = ev_tx.send(NetEvent::Connected);

        // Okuyucu görev
        let (rd_tx, mut rd_rx) = unbounded_channel::<ServerMsg>();
        let read_task = tokio::spawn(async move {
            loop {
                match protocol::read_frame_async(&mut read_half).await {
                    Ok(msg) => {
                        if rd_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = rd_tx.send(ServerMsg::Error {
                            text: format!("__disconnected__:{e}"),
                        });
                        break;
                    }
                }
            }
        });

        // 30 sn'de bir ping
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ping.tick().await;

        let disconnect_reason: Option<String>;
        loop {
            tokio::select! {
                maybe = out_rx.recv() => {
                    match maybe {
                        Some(msg) => {
                            if protocol::write_frame_async(&mut write_half, &msg).await.is_err() {
                                disconnect_reason = Some("Sunucuya yazılamadı".into());
                                break;
                            }
                        }
                        None => {
                            // UI kapatıldı / bağlantıyı kestik
                            disconnect_reason = Some("Kapatıldı".into());
                            break;
                        }
                    }
                }
                maybe = rd_rx.recv() => {
                    match maybe {
                        Some(ServerMsg::Error { text }) if text.starts_with("__disconnected__") => {
                            let reason = text.trim_start_matches("__disconnected__:").to_string();
                            disconnect_reason = Some(if reason.contains("eof") || reason.contains("closed") {
                                "Sunucu bağlantıyı kapattı".into()
                            } else {
                                format!("Bağlantı koptu: {reason}")
                            });
                            break;
                        }
                        Some(msg) => {
                            if ev_tx.send(NetEvent::Msg(msg)).is_err() {
                                disconnect_reason = Some("UI kapandı".into());
                                break;
                            }
                        }
                        None => {
                            disconnect_reason = Some("Okuyucu kapandı".into());
                            break;
                        }
                    }
                }
                _ = ping.tick() => {
                    if protocol::write_frame_async(&mut write_half, &ClientMsg::Ping).await.is_err() {
                        disconnect_reason = Some("Ping gönderilemedi".into());
                        break;
                    }
                }
            }
        }

        read_task.abort();
        flag.store(false, std::sync::atomic::Ordering::Relaxed);
        let _ = ev_tx.send(NetEvent::Disconnected {
            reason: disconnect_reason.unwrap_or_else(|| "Bağlantı bitti".into()),
        });
    });

    Client {
        out: out_tx,
        events: ev_rx,
        connected,
    }
}
