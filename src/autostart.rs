//! Starting with the computer: a launch agent on the Mac, the Run key on
//! Windows and an autostart entry on Linux. The app then starts hidden in
//! the menu bar / tray, ready to receive.
/// Command-line flag the entry starts the app with.
pub const HIDDEN_ARG: &str = "--hidden";

/// Add or remove the entry. Also called at every start while enabled, so the
/// entry follows the app when it has been moved.
pub fn set(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("找不到程序位置：{e}"))?;
    if enabled {
        add(&exe)
    } else {
        remove()
    }
}

#[cfg(target_os = "macos")]
fn agent_path() -> Result<std::path::PathBuf, String> {
    let home = directories::UserDirs::new().ok_or("找不到用户目录")?;
    Ok(home
        .home_dir()
        .join("Library/LaunchAgents/app.lantransfer.desktop.plist"))
}

#[cfg(target_os = "macos")]
fn add(exe: &std::path::Path) -> Result<(), String> {
    let exe = exe.to_string_lossy();
    // Opened straight from Downloads, macOS runs the app from a random
    // read-only copy that is gone after a restart.
    if exe.contains("/AppTranslocation/") {
        return Err("请先把邻传拖到“应用程序”文件夹并从那里打开，再开启此项。".into());
    }
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>app.lantransfer.desktop</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>{HIDDEN_ARG}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
</dict>
</plist>
"#,
        escape(&exe)
    );
    let path = agent_path()?;
    if std::fs::read_to_string(&path).ok().as_deref() == Some(plist.as_str()) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("无法设置开机启动：{e}"))?;
    }
    // Not loaded now: launchd would start a second copy right away. It is
    // picked up at the next login.
    std::fs::write(&path, plist).map_err(|e| format!("无法设置开机启动：{e}"))
}

#[cfg(target_os = "macos")]
fn remove() -> Result<(), String> {
    match std::fs::remove_file(agent_path()?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("无法取消开机启动：{e}")),
        _ => Ok(()),
    }
}

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const RUN_NAME: &str = "LanTransfer";

#[cfg(windows)]
fn add(exe: &std::path::Path) -> Result<(), String> {
    let command = format!("\"{}\" {HIDDEN_ARG}", exe.display());
    if crate::registry::set(RUN_KEY, RUN_NAME, &command) {
        Ok(())
    } else {
        Err("无法设置开机启动：写入注册表失败".into())
    }
}

#[cfg(windows)]
fn remove() -> Result<(), String> {
    crate::registry::delete(RUN_KEY, RUN_NAME);
    Ok(())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn entry_path() -> Result<std::path::PathBuf, String> {
    let dirs = directories::BaseDirs::new().ok_or("找不到用户目录")?;
    Ok(dirs.config_dir().join("autostart/lan-transfer.desktop"))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn add(exe: &std::path::Path) -> Result<(), String> {
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=邻传\nExec=\"{}\" {HIDDEN_ARG}\n",
        exe.display()
    );
    let path = entry_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("无法设置开机启动：{e}"))?;
    }
    std::fs::write(&path, entry).map_err(|e| format!("无法设置开机启动：{e}"))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn remove() -> Result<(), String> {
    match std::fs::remove_file(entry_path()?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("无法取消开机启动：{e}")),
        _ => Ok(()),
    }
}
