use std::{
    ffi::CString,
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, channel},
    },
    time::Duration,
};

use jni::{JavaVM, JValue};
use serde_json::{Value, json};

/// build.rs: javac başarılı mı?
fn java_compiled() -> bool {
    env!("RUSTCHAT_JAVA_OK") == "1"
}

const CLASS_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/classes/com/rustchat/bridge/MessageProcessor.class"
));

const CLASS_NAME: &str = "com/rustchat/bridge/MessageProcessor";
const REPLY_TIMEOUT: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Disabled,
    Starting,
    Active,
    Failed(String),
}

enum Request {
    Process {
        text: String,
        reply: Sender<String>,
    },
    Hint {
        text: String,
        reply: Sender<String>,
    },
}

/// Java mesaj işleme modülüne erişim. Kullanılamazsa Rust fallback devreye girer.
pub struct Bridge {
    tx: Option<Sender<Request>>,
    status: Arc<Mutex<Status>>,
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridge {
    pub fn new() -> Self {
        let status = Arc::new(Mutex::new(if java_compiled() {
            Status::Starting
        } else {
            Status::Disabled
        }));

        if !java_compiled() {
            return Self { tx: None, status };
        }

        let (tx, rx) = channel::<Request>();
        {
            let status = status.clone();
            std::thread::Builder::new()
                .name("jvm-host".into())
                .spawn(move || jvm_host(rx, status))
                .ok();
        }

        Self {
            tx: Some(tx),
            status,
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn is_active(&self) -> bool {
        matches!(*self.status.lock().unwrap(), Status::Active)
    }

    /// Giden mesajı işler; dönen değer geçerli JSON metnidir.
    pub fn process(&self, text: &str) -> String {
        if let Some(res) = self.try_java(text, true) {
            return res;
        }
        fallback_process(text).to_string()
    }

    /// Yarım komut için kısa ipucu (yoksa boş string).
    pub fn hint(&self, text: &str) -> String {
        if let Some(res) = self.try_java(text, false) {
            return res;
        }
        fallback_hint(text)
    }

    fn try_java(&self, text: &str, is_process: bool) -> Option<String> {
        if !self.is_active() {
            return None;
        }
        let tx = self.tx.as_ref()?;
        let (rtx, rrx) = channel::<String>();
        let req = if is_process {
            Request::Process {
                text: text.to_string(),
                reply: rtx,
            }
        } else {
            Request::Hint {
                text: text.to_string(),
                reply: rtx,
            }
        };
        if tx.send(req).is_err() {
            self.mark_failed("kanal kapandı".into());
            return None;
        }
        match rrx.recv_timeout(REPLY_TIMEOUT) {
            Ok(s) if !s.is_empty() => Some(s),
            Ok(_) => {
                // Boş yanıt: ipucu yok (hint) ya da hata (process)
                if is_process {
                    self.mark_failed("Java çağrısı başarısız".into());
                }
                None
            }
            Err(_) => {
                self.mark_failed("yanıt zaman aşımı".into());
                None
            }
        }
    }

    fn mark_failed(&self, reason: String) {
        let mut st = self.status.lock().unwrap();
        if matches!(*st, Status::Active) {
            *st = Status::Failed(reason);
        }
    }
}

// ---------------------------------------------------------------- JVM host

fn jvm_host(rx: Receiver<Request>, status: Arc<Mutex<Status>>) {
    let result = (|| -> Result<JavaVM, String> {
        let jvm_dll = find_jvm_dll().ok_or("jvm.dll bulunamadı (JDK kurulu mu?)")?;
        let dir = jvm_dll.parent().ok_or("geçersiz yol")?;

        // JVM'in bağımlı DLL'lerini bulabilmesi için PATH'e ekle
        let bin_dir = dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| dir.to_path_buf());
        unsafe {
            let key = "PATH";
            let mut old = std::env::var_os(key).unwrap_or_default();
            old.push(";");
            old.push(&bin_dir);
            old.push(";");
            old.push(dir);
            std::env::set_var(key, old);
        }

        // Sınıf dosyasını hazırla
        let classes_dir = extract_classes()?;

        unsafe {
            let lib = libloading::Library::new(&jvm_dll)
                .map_err(|e| format!("jvm.dll yüklenemedi: {e}"))?;
            type CreateVm = unsafe extern "system" fn(
                *mut *mut std::ffi::c_void,
                *mut *mut std::ffi::c_void,
                *mut std::ffi::c_void,
            ) -> jni_sys::jint;
            let create: libloading::Symbol<CreateVm> = lib
                .get(b"JNI_CreateJavaVM")
                .map_err(|e| format!("JNI_CreateJavaVM yok: {e}"))?;

            let cp = format!("-Djava.class.path={}", classes_dir.display());
            let cp_c = CString::new(cp).map_err(|e| e.to_string())?;
            let options = [jni_sys::JavaVMOption {
                optionString: cp_c.as_ptr() as *mut _,
                extraInfo: std::ptr::null_mut(),
            }];

            let mut init_args = jni_sys::JavaVMInitArgs {
                version: jni_sys::JNI_VERSION_1_8,
                nOptions: options.len() as jni_sys::jint,
                options: options.as_ptr() as *mut _,
                ignoreUnrecognized: jni_sys::JNI_FALSE as _,
            };

            let mut vm_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let mut env_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let rc = create(&mut vm_ptr, &mut env_ptr, &mut init_args as *mut _ as _);
            if rc != jni_sys::JNI_OK || vm_ptr.is_null() {
                return Err(format!("JNI_CreateJavaVM başarısız (kod {rc})"));
            }
            std::mem::forget(lib);

            let vm = JavaVM::from_raw(vm_ptr as *mut jni_sys::JavaVM);
            Ok(vm)
        }
    })();

    match result {
        Ok(vm) => {
            *status.lock().unwrap() = Status::Active;
            host_loop(&vm, rx);
        }
        Err(reason) => {
            *status.lock().unwrap() = Status::Failed(reason);
            // İstekleri yut, çağrılar fallback'e düşsün
            while let Ok(_req) = rx.recv() {}
        }
    }
}

fn host_loop(vm: &JavaVM, rx: Receiver<Request>) {
    while let Ok(req) = rx.recv() {
        let (method, text, reply) = match req {
            Request::Process { text, reply } => ("process", text, reply),
            Request::Hint { text, reply } => ("hint", text, reply),
        };
        match call_java(vm, method, &text) {
            Ok(json_str) => {
                let _ = reply.send(json_str);
            }
            Err(e) => {
                let _ = reply.send(String::new());
                let _ = e;
                break;
            }
        }
    }
}

fn call_java(vm: &JavaVM, method: &str, text: &str) -> Result<String, String> {
    use jni::{
        objects::JString,
        signature::RuntimeMethodSignature,
        strings::JNIString,
    };
    let sig = RuntimeMethodSignature::from_str("(Ljava/lang/String;)Ljava/lang/String;")
        .map_err(|e| format!("imza: {e}"))?;
    let sig = sig.method_signature();
    vm.attach_current_thread(|env| {
        let cls = env.find_class(JNIString::new(CLASS_NAME))?;
        let jstr = env.new_string(text)?;
        let ret = env.call_static_method(&cls, JNIString::new(method), &sig, &[JValue::Object(&jstr)])?;
        let obj = ret.l()?;
        let js = unsafe { JString::from_raw(env, obj.as_raw()) };
        let s = js.try_to_string(env)?;
        Ok::<String, jni::errors::Error>(s)
    })
    .map_err(|e| format!("jni: {e}"))
}

fn find_jvm_dll() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(home) = std::env::var("JAVA_HOME") {
        let h = PathBuf::from(home);
        candidates.push(h.join("bin/server/jvm.dll"));
        candidates.push(h.join("jre/bin/server/jvm.dll"));
    }

    if let Ok(out) = Command::new("where.exe").arg("java.exe").output() {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let exe = PathBuf::from(line.trim());
                if let Some(bin) = exe.parent() {
                    if let Some(home) = bin.parent() {
                        candidates.push(home.join("bin/server/jvm.dll"));
                    }
                    candidates.push(bin.join("server/jvm.dll"));
                }
            }
        }
    }

    for base in [
        r"C:\Program Files\Java",
        r"C:\Program Files (x86)\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files\Eclipse Foundation",
        r"C:\Program Files\Microsoft",
        r"C:\Program Files\Amazon Corretto",
    ] {
        if let Ok(entries) = std::fs::read_dir(base) {
            let mut homes: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            homes.sort();
            homes.reverse();
            for h in homes {
                candidates.push(h.join("bin/server/jvm.dll"));
            }
        }
    }

    candidates.into_iter().find(|p| p.exists())
}

fn extract_classes() -> Result<PathBuf, String> {
    if CLASS_BYTES.is_empty() {
        return Err("Java modülü derlenmemiş".into());
    }
    let dir = std::env::temp_dir().join(format!("rustchat-classes-{}", class_hash()));
    let file = dir.join("com/rustchat/bridge/MessageProcessor.class");
    let need_write = std::fs::read(&file)
        .map(|old| old != CLASS_BYTES)
        .unwrap_or(true);
    if need_write {
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&file, CLASS_BYTES).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

fn class_hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in CLASS_BYTES {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

// ------------------------------------------------------- Rust fallback

pub fn fallback_process(text: &str) -> Value {
    let trimmed = text.trim();
    if trimmed.starts_with('/') && trimmed.len() > 1 {
        let rest_all = match trimmed.find(char::is_whitespace) {
            Some(i) => trimmed[i..].trim(),
            None => "",
        };
        let cmd = trimmed[1..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        let args: Vec<&str> = rest_all.split_whitespace().collect();
        return json!({
            "kind": "command",
            "cmd": cmd,
            "args": args,
            "rest": rest_all,
        });
    }

    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    let emojis = text.chars().filter(|c| is_emoji(*c as u32)).count();
    let mut links = Vec::new();
    let mut mentions = Vec::new();
    let mut hashtags = Vec::new();
    scan_tokens(text, &mut links, &mut mentions, &mut hashtags);

    json!({
        "kind": "message",
        "text": text,
        "stats": {
            "words": words,
            "chars": chars,
            "emojis": emojis,
            "links": links,
            "mentions": mentions,
            "hashtags": hashtags,
        }
    })
}

pub fn fallback_hint(text: &str) -> String {
    if !text.starts_with('/') {
        return String::new();
    }
    let head = text[1..].split_whitespace().next().unwrap_or("").to_lowercase();
    match head.as_str() {
        "join" | "j" => "/join <oda> - odaya katıl".into(),
        "msg" => "/msg <kisi> <mesaj> - özel mesaj".into(),
        "me" => "/me <eylem> - rol eylemi".into(),
        "nick" => "/nick <yeni_isim> - isim değiştir".into(),
        "clear" => "/clear - konuşmayı temizle".into(),
        "help" => "/help - komut listesi".into(),
        "quit" => "/quit - bağlantıyı kes".into(),
        _ => String::new(),
    }
}

fn is_emoji(cp: u32) -> bool {
    (0x1F300..=0x1FAFF).contains(&cp)
        || (0x1F000..=0x1F2FF).contains(&cp)
        || (0x2600..=0x27BF).contains(&cp)
        || (0x2B00..=0x2BFF).contains(&cp)
        || (0x1F1E6..=0x1F1FF).contains(&cp)
        || matches!(cp, 0x2764 | 0x2705 | 0x274C | 0x2B50 | 0x203C | 0x2049)
}

fn scan_tokens(text: &str, links: &mut Vec<String>, mentions: &mut Vec<String>, hashtags: &mut Vec<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        if rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("www.") {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            links.push(rest[..end].to_string());
            i += end;
        } else if rest.starts_with('@') {
            let name: String = text[i + 1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect();
            if !name.is_empty() {
                let len = name.len();
                mentions.push(name);
                i += 1 + len;
            } else {
                i += 1;
            }
        } else if rest.starts_with('#') {
            let name: String = text[i + 1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .collect();
            if !name.is_empty() {
                let len = name.len();
                hashtags.push(name);
                i += 1 + len;
            } else {
                i += 1;
            }
        } else {
            let ch_len = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            i += ch_len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_command() {
        let v = fallback_process("/join rust lobi");
        assert_eq!(v["kind"], "command");
        assert_eq!(v["cmd"], "join");
        assert_eq!(v["args"][0], "rust");
        assert_eq!(v["rest"], "rust lobi");
    }

    #[test]
    fn fallback_message_stats() {
        let v = fallback_process("merhaba dünya https://rust-lang.org @ali #rust 🔥");
        assert_eq!(v["kind"], "message");
        assert_eq!(v["stats"]["words"], 6);
        assert_eq!(v["stats"]["emojis"], 1);
        assert_eq!(v["stats"]["links"][0], "https://rust-lang.org");
        assert_eq!(v["stats"]["mentions"][0], "ali");
        assert_eq!(v["stats"]["hashtags"][0], "rust");
    }

    #[test]
    fn hint_fallback() {
        assert!(fallback_hint("/jo").is_empty() || fallback_hint("/jo").starts_with("/join"));
        assert!(fallback_hint("/join x").contains("odaya"));
        assert_eq!(fallback_hint("merhaba"), "");
    }
}
