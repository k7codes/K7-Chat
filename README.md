# 🦀 RustChat

> Rust ile yazılmış, **egui/eframe** tabanlı modern bir masaüstü sohbet uygulaması.
> TCP üzerinden kendi sunucusunu başlatabilir, odalarda ve özel mesajlarda gerçek zamanlı
> konuşabilirsin. Giden mesajlar istege bağlı olarak gömülü bir **Java (JNI)** modülünden geçirilir.

![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?logo=rust)
![egui](https://img.shields.io/badge/GUI-egui%2Feframe-blue)
![Java](https://img.shields.io/badge/Java-optional-red?logo=openjdk)
![Platform](https://img.shields.io/badge/platform-Windows-informational)
![Status](https://img.shields.io/badge/selftest-13%2F13_geçti-brightgreen)

---

## ✨ Özellikler

- **İstemci + Sunucu tek uygulamada** — sekmeler arası geçiş ile sunucu başlat, istemciyle bağlan.
- **Gerçek zamanlı sohbet** — odalar (`#genel`, `#rust`, `#rasgele`), özel mesajlar, sistem bildirimleri.
- **Zengin arayüz** — animasyonlu mesaj baloncukları, yazıyor göstergesi, okundu/teslim tikleri (`✓` / `✓✓`), okunmamış rozetleri.
- **4 tema** — Neon, Sunset, Ocean, Matrix. Renkler arasında yumuşak geçiş animasyonu.
- **Java köprüsü (JNI)** — mesajları Java modülü işler; Java yoksa otomatik **Rust fallback** devreye girer.
- **Mesaj analizi** — kelime/karakter/emoji sayımı, `@mention`, `#hashtag` ve link tespiti.
- **Emoji seçici** ve komut ipuçları (`/help`).
- **Kendi kendini test etme** — `--selftest` ile protokol, Java/JNI, sunucu ve uçtan uca akış doğrulanır.

---

## 📸 Ekran Görüntüleri

> `docs/screenshots/` klasörüne uygulama ekran görüntülerini ekleyip buraya bağlayabilirsin.

| İstemci | Sunucu Paneli |
| :---: | :---: |
| ![client](docs/screenshots/client.png) | ![server](docs/screenshots/server.png) |

---

## 🚀 Kurulum

### Gereksinimler

| Bileşen | Sürüm | Zorunlu mu? |
| --- | --- | --- |
| [Rust](https://rustup.rs/) | `1.99+` (`edition 2024`) | ✅ Evet |
| JDK / JRE | 8+ (`javac` ve `jvm.dll`) | ⚠️ Opsiyonel |
| C++ Build Tools | MSVC (Windows) | ✅ Evet |

> Java kurulu değilse uygulama yine çalışır; mesaj işleme Rust tarafındaki fallback'e düşer.

### Derleme ve Çalıştırma

```bash
# Geliştirme modu
cargo run

# Yayın (release) derlemesi
cargo build --release
./target/release/RustChat.exe
```

`build.rs`, `java/src` altındaki kaynakları `javac` ile derleyip sınıf dosyasını
uygulamanın içine gömüyor. `javac` bulunamazsa Java modülü otomatik devre dışı bırakılır.

---

## 🧪 Testler

```bash
# Birim testleri
cargo test

# Uçtan uca kendi kendini test (protokol + JNI + sunucu/istemci)
cargo run -- --selftest
```

`--selftest` sonucu konsola ve `selftest.log` dosyasına yazılır:

```
RustChat selftest
==================================================
[PASS] protokol.frame_roundtrip
[PASS] fallback.command
[PASS] java.status - Active
[PASS] server.start - port=57920
[PASS] e2e.echo
...
SONUÇ: TÜM TESTLER GEÇTİ
```

---

## 🖥️ Kullanım

1. `cargo run` ile uygulamayı aç.
2. **Sunucu** sekmesinden bir port seçip **Başlat**'a bas (varsayılan `5000`).
3. **İstemci** sekmesine geç, adres `127.0.0.1`, port ve kullanıcı adını girip **Bağlan**'a bas.
4. İkinci bir örnek açarak (ya da başka bir makineden) aynı sunucuya bağlanıp sohbet et.

### Komutlar

| Komut | Açıklama |
| --- | --- |
| `/join <oda>` · `/j <oda>` | Bir odaya katıl |
| `/msg <kişi> <mesaj>` | Özel mesaj gönder |
| `/me <eylem>` | Rol eylemi (italik) |
| `/nick <yeni_isim>` | Kullanıcı adını değiştir |
| `/clear` | Aktif konuşmayı temizle |
| `/help` | Komut listesi |
| `/quit` | Bağlantıyı kes |

---

## 🏗️ Mimari

```
┌──────────────────────────────────────────────────────────┐
│                        RustChat (GUI)                     │
│                                                           │
│   ┌────────────┐   ┌──────────────┐   ┌───────────────┐  │
│   │  İstemci   │   │    Sunucu     │   │  Java Köprüsü │  │
│   │  (net.rs)  │◄─►│ (server.rs)   │   │(java_bridge)  │  │
│   └─────┬──────┘   └──────┬───────┘   └───────┬───────┘  │
│         │                 │                    │ JNI       │
│         ▼                 ▼                    ▼           │
│    TCP soket        TCP listener        MessageProcessor   │
│    (JSON frame)     (JSON frame)        (Java, gömülü)     │
│                                                │           │
│                                          yoksa ▼           │
│                                     Rust fallback          │
└──────────────────────────────────────────────────────────┘
```

### Protokol

- Her mesaj **4 byte big-endian uzunluk + JSON** olarak çerçevelenir (max `1 MiB`).
- İstemci ilk olarak `Hello { name }` gönderir; sunucu `Welcome` ve geçmişi yanıtlar.
- İstemci 30 sn'de bir `Ping` atar; sunucu `Pong` döner.

**İstemci → Sunucu:** `Hello`, `Say`, `JoinRoom`, `OpenDm`, `Typing`, `Read`, `Rename`, `Ping`
**Sunucu → İstemci:** `Welcome`, `History`, `Users`, `Rooms`, `Chat`, `UserJoined`, `UserLeft`, `Typing`, `ReadBy`, `Error`, `Pong`

---

## 📁 Proje Yapısı

```
Rust Chat/
├── Cargo.toml            # Bağımlılıklar ve release profili
├── build.rs              # javac ile Java kaynaklarını derler, .class'ı gömer
├── java/
│   └── src/com/rustchat/bridge/
│       └── MessageProcessor.java   # Mesaj işleme / istatistik (JNI hedefi)
└── src/
    ├── main.rs           # Giriş noktası + --selftest
    ├── protocol.rs       # Mesaj türleri, çerçeveleme, doğrulama
    ├── net.rs            # Tokio tabanlı istemci, paylaşılan runtime
    ├── server.rs         # Hub, odalar, DM'ler, yayın mantığı
    ├── state.rs          # İstemci UI durumu ve olay işleme
    ├── theme.rs          # 4 tema + geçiş animasyonu
    ├── java_bridge.rs    # JNI köprüsü ve Rust fallback
    └── ui/
        ├── mod.rs        # Uygulama kabuğu, üst bar, sekmeler
        ├── connect.rs    # Bağlantı ekranı
        ├── chat.rs       # Sohbet paneli, baloncuklar, analiz şeridi
        ├── serverpanel.rs# Sunucu kontrol paneli + günlük
        └── widgets.rs    # Çizim yardımcıları (glow, gradient, chip...)
```

---

## 🔧 Teknolojiler

- **[eframe / egui](https://github.com/emilk/egui)** — anlık mod (immediate-mode) GUI
- **[tokio](https://tokio.rs/)** — asenkron TCP ağı
- **[serde](https://serde.rs/) / serde_json** — çerçeve serileştirme
- **[jni](https://crates.io/crates/jni) / libloading** — gömülü JVM entegrasyonu
- **[chrono](https://crates.io/crates/chrono)** — zaman damgaları

---

## 📝 Notlar

- Ağ katmanında uygulama başına tek bir Tokio runtime kullanılır (UI + sunucu + istemci).
- Kullanıcı adı, oda adı ve mesaj metinleri sunucu tarafında temizlenir ve sınırlandırılır.
- Oda geçmişi `500`, DM geçmişi `200` mesajla sınırlıdır.

---

## 📄 Lisans

Bu proje için henüz bir lisans belirlenmedi. Lisans eklemek istersen
[MIT](https://choosealicense.com/licenses/mit/) veya [Apache-2.0](https://choosealicense.com/licenses/apache-2.0/)
gibi bir dosya (`LICENSE`) ekleyip bu bölümü güncelleyebilirsin.
