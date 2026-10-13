#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod java_bridge;
mod net;
mod protocol;
mod server;
mod state;
mod theme;
mod ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--selftest") {
        let ok = selftest::run();
        std::process::exit(if ok { 0 } else { 1 });
    }
    ui::run();
}

mod selftest {
    use std::io::Write;
    use std::time::{Duration, Instant};

    use crate::java_bridge::Bridge;
    use crate::net::{NetEvent, self as net};
    use crate::protocol::{ClientMsg, Scope, ServerMsg, read_frame_sync};

    pub fn run() -> bool {
        let mut results: Vec<(&'static str, bool, String)> = Vec::new();

        // 1) Protokol çerçeve tur-önü-geri
        {
            let msg = ClientMsg::Say {
                scope: Scope::Room { room: "genel".into() },
                text: "selam 🚀".into(),
                action: false,
            };
            let bytes = crate::protocol::encode_frame(&msg);
            let mut cur = std::io::Cursor::new(bytes);
            let back: ClientMsg = read_frame_sync(&mut cur).unwrap_or(ClientMsg::Ping);
            let ok = matches!(&back, ClientMsg::Say { text, .. } if text == "selam 🚀");
            results.push(("protokol.frame_roundtrip", ok, format!("{back:?}")));
        }

        // 2) Java fallback (aynı JSON sözleşmesi)
        {
            let v = crate::java_bridge::fallback_process("/join rust lobi");
            let ok = v["kind"] == "command" && v["cmd"] == "join" && v["rest"] == "rust lobi";
            results.push(("fallback.command", ok, v.to_string()));
            let v = crate::java_bridge::fallback_process("merhaba dünya @ali");
            let ok = v["kind"] == "message" && v["stats"]["words"] == 3;
            results.push(("fallback.stats", ok, v.to_string()));
        }

        // 3) Java/JNI modülü
        {
            let bridge = Bridge::new();
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline
                && !matches!(bridge.status(), crate::java_bridge::Status::Active)
            {
                if let crate::java_bridge::Status::Failed(e) = bridge.status() {
                    results.push(("java.status", false, e));
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let active = bridge.is_active();
            results.push(("java.status", active, format!("{:?}", bridge.status())));
            if active {
                let raw = bridge.process("merhaba dünya 🔥");
                let v: serde_json::Value = serde_json::from_str(&raw)
                    .unwrap_or(serde_json::Value::Null);
                let ok = v["kind"] == "message"
                    && v["stats"]["words"] == 3
                    && v["stats"]["emojis"] == 1;
                results.push(("java.process_message", ok, raw.clone()));

                let raw = bridge.process("/msg ali naber");
                let v: serde_json::Value = serde_json::from_str(&raw)
                    .unwrap_or(serde_json::Value::Null);
                let ok = v["kind"] == "command"
                    && v["cmd"] == "msg"
                    && v["args"][0] == "ali"
                    && v["rest"] == "ali naber";
                results.push(("java.process_command", ok, raw.clone()));

                let h = bridge.hint("/join");
                results.push(("java.hint", h.contains("odaya"), h));
            }
        }

        // 4) Uçtan uca: sunucu + istemci
        {
            match crate::server::start(0) {
                Ok(mut handle) => {
                    let port = handle.port;
                    results.push(("server.start", port > 0, format!("port={port}")));

                    let client = net::connect(format!("127.0.0.1:{port}"));
                    client.send(ClientMsg::Hello {
                        name: "tester".into(),
                    });

                    let deadline = Instant::now() + Duration::from_secs(6);
                    let mut got_welcome = false;
                    let mut got_echo = false;
                    let mut got_history = false;
                    let mut sent_say = false;
                    while Instant::now() < deadline && !(got_welcome && got_echo) {
                        match client.events.recv_timeout(Duration::from_millis(200)) {
                            Ok(NetEvent::Msg(ServerMsg::Welcome { name, .. })) => {
                                got_welcome = name == "tester";
                                if got_welcome && !sent_say {
                                    sent_say = true;
                                    client.send(ClientMsg::Say {
                                        scope: Scope::Room { room: "genel".into() },
                                        text: "uçtan uca test".into(),
                                        action: false,
                                    });
                                }
                            }
                            Ok(NetEvent::Msg(ServerMsg::History { .. })) => {
                                got_history = true;
                            }
                            Ok(NetEvent::Msg(ServerMsg::Chat(m))) => {
                                if m.text == "uçtan uca test" && m.from == "tester" {
                                    got_echo = true;
                                }
                            }
                            Ok(NetEvent::Msg(_)) => {}
                            Ok(NetEvent::Connected) => {}
                            Ok(NetEvent::Disconnected { reason }) => {
                                results.push(("client.disconnect", false, reason));
                                break;
                            }
                            Err(_) => {}
                        }
                    }
                    results.push(("client.welcome", got_welcome, String::new()));
                    results.push(("client.history", got_history, String::new()));
                    results.push(("e2e.echo", got_echo, String::new()));

                    // Sunucu günlük olayları
                    let mut ev_count = 0;
                    while handle.events.try_recv().is_ok() {
                        ev_count += 1;
                    }
                    results.push(("server.events", ev_count > 0, format!("{ev_count} olay")));

                    drop(client);
                    handle.stop();
                    results.push(("server.stop", true, String::new()));
                }
                Err(e) => results.push(("server.start", false, e.to_string())),
            }
        }

        // Yazdır
        let mut file = std::fs::File::create("selftest.log").ok();
        let mut all_ok = true;
        let mut out = String::new();
        out.push_str("RustChat selftest\n");
        out.push_str(&"=".repeat(50));
        out.push('\n');
        for (name, ok, detail) in &results {
            if !ok {
                all_ok = false;
            }
            out.push_str(&format!(
                "[{}] {} {}\n",
                if *ok { "PASS" } else { "FAIL" },
                name,
                if detail.is_empty() {
                    String::new()
                } else {
                    format!("- {detail}")
                }
            ));
        }
        out.push_str(&"=".repeat(50));
        out.push('\n');
        out.push_str(if all_ok {
            "SONUÇ: TÜM TESTLER GEÇTİ\n"
        } else {
            "SONUÇ: HATA VAR\n"
        });
        println!("{out}");
        if let Some(f) = file.as_mut() {
            let _ = f.write_all(out.as_bytes());
        }
        all_ok
    }
}
