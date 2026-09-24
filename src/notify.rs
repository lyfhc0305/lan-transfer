//! System notifications (macOS Notification Center, Windows toasts) for
//! things that happen while the window is hidden. Best effort: failures are
//! ignored, the transfer list is the record.
use std::sync::atomic::{AtomicBool, Ordering};

/// Set after the first failure so a missing notification service does not
/// cost a thread per event.
static BROKEN: AtomicBool = AtomicBool::new(false);

pub fn system(title: &str, body: &str) {
    if BROKEN.load(Ordering::Relaxed) {
        return;
    }
    let (title, body) = (title.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        let result = notify_rust::Notification::new()
            .summary(&title)
            .body(&body)
            .appname("邻传")
            .show();
        if result.is_err() {
            BROKEN.store(true, Ordering::Relaxed);
        }
    });
}
