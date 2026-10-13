use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=java/src");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let classes = out_dir.join("classes");
    let class_file = classes.join("com/rustchat/bridge/MessageProcessor.class");

    let mut sources = Vec::new();
    collect_java(PathBuf::from("java/src"), &mut sources);

    let mut ok = false;
    if !sources.is_empty() {
        if let Some(javac) = find_javac() {
            let status = Command::new(&javac)
                .arg("-d")
                .arg(&classes)
                .args(&sources)
                .status();
            ok = matches!(status, Ok(s) if s.success());
            if !ok {
                println!("cargo:warning=javac compilation failed; Java module will be disabled");
            }
        } else {
            println!("cargo:warning=javac not found; Java module will be disabled");
        }
    }

    if !class_file.exists() {
        std::fs::create_dir_all(class_file.parent().unwrap()).ok();
        std::fs::write(&class_file, b"").ok();
        ok = false;
    }

    println!("cargo:rustc-env=RUSTCHAT_JAVA_OK={}", if ok { "1" } else { "0" });
}

fn collect_java(dir: PathBuf, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java(path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path);
        }
    }
}

fn find_javac() -> Option<PathBuf> {
    if let Ok(home) = env::var("JAVA_HOME") {
        let p = PathBuf::from(&home).join("bin/javac.exe");
        if p.exists() {
            return Some(p);
        }
    }
    let output = Command::new("where.exe").arg("javac").output().ok()?;
    if output.status.success() {
        let first = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .trim()
            .to_string();
        if !first.is_empty() {
            return Some(PathBuf::from(first));
        }
    }
    None
}
