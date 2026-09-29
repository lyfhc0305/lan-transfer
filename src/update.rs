//! Checking GitHub Releases for a newer version, once a day.
use crate::model::{Event, Shared};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

const LATEST: &str = "https://api.github.com/repos/lyfhc0305/lan-transfer/releases/latest";

/// A newer version that can be downloaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub fn start(shared: Arc<Shared>) {
    std::thread::spawn(move || {
        // Let the window and the network start first.
        std::thread::sleep(Duration::from_secs(10));
        loop {
            if shared.settings.lock().unwrap().check_updates {
                check(&shared);
            }
            std::thread::sleep(Duration::from_secs(24 * 3600));
        }
    });
}

/// Ask GitHub for the latest release and announce it once when it is newer.
pub fn check(shared: &Shared) {
    let Some(release) = latest().filter(|r| newer(&r.version, env!("CARGO_PKG_VERSION"))) else {
        return;
    };
    let mut known = shared.update.lock().unwrap();
    if known.as_ref() == Some(&release) {
        return;
    }
    *known = Some(release);
    drop(known);
    shared.event(Event::Update);
}

/// Uses the system's curl (built into macOS and Windows 10 and later), so the
/// app carries no HTTPS stack of its own.
fn latest() -> Option<Release> {
    let mut command = std::process::Command::new("curl");
    command.args([
        "-fsSL",
        "--max-time",
        "15",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        concat!("User-Agent: LanTransfer/", env!("CARGO_PKG_VERSION")),
        LATEST,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok().filter(|o| o.status.success())?;
    parse(&output.stdout)
}

fn parse(json: &[u8]) -> Option<Release> {
    let latest: Latest = serde_json::from_slice(json).ok()?;
    if latest.draft || latest.prerelease || !latest.html_url.starts_with("https://github.com/") {
        return None;
    }
    let version = latest.tag_name.trim_start_matches(['v', 'V']).to_owned();
    Some(Release {
        version,
        url: latest.html_url,
    })
}

/// "0.4.0" is newer than "0.3.0"; anything after "-" or "+" is ignored.
fn newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> {
        v.split(['-', '+'])
            .next()?
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<Vec<u64>>>()
            .map(|mut v| {
                while v.last() == Some(&0) {
                    v.pop();
                }
                v
            })
    };
    match (parts(candidate), parts(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_releases() {
        assert!(newer("0.4.0", "0.3.0"));
        assert!(newer("0.3.10", "0.3.9"));
        assert!(newer("1.0", "0.9.9"));
        assert!(!newer("0.3.0", "0.3.0"));
        assert!(!newer("0.4.0", "0.4"));
        assert!(!newer("0.2.9", "0.3.0"));
        assert!(!newer("最新", "0.3.0"));
        let json = br#"{"tag_name":"v0.4.0","html_url":"https://github.com/lyfhc0305/lan-transfer/releases/tag/v0.4.0","draft":false,"prerelease":false}"#;
        assert_eq!(
            parse(json),
            Some(Release {
                version: "0.4.0".into(),
                url: "https://github.com/lyfhc0305/lan-transfer/releases/tag/v0.4.0".into(),
            })
        );
        let beta =
            br#"{"tag_name":"v0.5.0-beta","html_url":"https://github.com/x","prerelease":true}"#;
        assert_eq!(parse(beta), None);
    }
}
