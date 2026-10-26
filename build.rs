fn main() {
    println!("cargo:rerun-if-changed=packaging/app.rc");
    println!("cargo:rerun-if-changed=packaging/app.manifest");
    println!("cargo:rerun-if-changed=assets/AppIcon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let target = std::env::var("TARGET").unwrap();
        if target.ends_with("-gnu") || target.ends_with("-gnullvm") {
            let output =
                std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("app-res.o");
            let program = std::env::var_os("WINDRES").unwrap_or_else(|| "windres".into());
            let status = std::process::Command::new(program)
                .args(["-i", "packaging/app.rc", "-o"])
                .arg(&output)
                .args(["-O", "coff", "-I", "."])
                .status()
                .expect("Windows resource compiler is required");
            assert!(status.success(), "Windows resource compilation failed");
            println!("cargo:rustc-link-arg={}", output.display());
        }
    }
}
