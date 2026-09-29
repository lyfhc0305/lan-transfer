//! System notifications, for transfers that end while the window is hidden
//! in the menu bar / tray or behind other windows.
use crate::model::Shared;

/// Show a notification; failures are ignored (the notice inside the window
/// is still shown when it comes back).
pub fn show(shared: &Shared, title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    #[cfg(windows)]
    {
        let me = shared.me();
        std::thread::spawn(move || windows::show(&title, &body, me));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = shared;
        mac::show(&title, &body);
    }
    #[cfg(target_os = "linux")]
    {
        let _ = shared;
        // Development only: the release targets are macOS and Windows.
        let _ = std::process::Command::new("notify-send")
            .args(["--app-name=邻传", &title, &body])
            .spawn();
    }
}

// UNUserNotificationCenter, the replacement, needs a signed app.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
mod mac {
    use objc2_foundation::{NSBundle, NSString, NSUserNotification, NSUserNotificationCenter};

    pub fn show(title: &str, body: &str) {
        // Without a bundle (running the bare binary) there is no default
        // centre; the call would return nil.
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            return;
        }
        let n = NSUserNotification::new();
        n.setTitle(Some(&NSString::from_str(title)));
        n.setInformativeText(Some(&NSString::from_str(body)));
        NSUserNotificationCenter::defaultUserNotificationCenter().deliverNotification(&n);
    }
}

#[cfg(windows)]
mod windows {
    use crate::{
        model::{config_dir, Shared},
        registry,
    };
    use std::sync::{Once, Weak};
    use tauri_winrt_notification::Toast;

    const APP_ID: &str = "app.lantransfer.desktop";

    pub fn show(title: &str, body: &str, me: Weak<Shared>) {
        static REGISTER: Once = Once::new();
        REGISTER.call_once(register);
        let _ = Toast::new(APP_ID)
            .title(title)
            .text1(body)
            .on_activated(move |_| {
                if let Some(shared) = me.upgrade() {
                    shared.show();
                }
                Ok(())
            })
            .show();
    }

    /// A portable app has no Start menu shortcut to name it, so Windows
    /// would drop its toasts. Registering the app ID gives them a name and
    /// icon.
    fn register() {
        let key = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
        registry::set(&key, "DisplayName", "邻传");
        let icon = config_dir().join("AppIcon.ico");
        let data = include_bytes!("../assets/AppIcon.ico");
        if std::fs::read(&icon).ok().as_deref() != Some(&data[..]) {
            let _ = std::fs::create_dir_all(config_dir());
            let _ = std::fs::write(&icon, data);
        }
        registry::set(&key, "IconUri", &icon.to_string_lossy());
    }
}
