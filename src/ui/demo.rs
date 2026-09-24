//! Fake devices and transfers for screenshots (`--features demo`).
//!
//! `LAN_TRANSFER_DEMO=<scene>` picks what to show, `LAN_TRANSFER_THEME=dark`
//! forces the dark appearance. Nothing is sent or received.
use super::*;

fn peer(n: u8, name: &str, platform: Platform, ip: &str) -> Peer {
    Peer {
        id: format!("{n:02x}").repeat(32),
        name: name.into(),
        platform,
        address: format!("{ip}:{PORT}").parse().unwrap(),
        seen: Instant::now(),
    }
}

fn transfer(outgoing: bool, peer: &str, items: &[&str], files: usize, total: u64) -> Transfer {
    let mut t = Transfer::new(outgoing, peer, "192.168.1.40");
    t.items = items.iter().map(|s| s.to_string()).collect();
    t.files = files;
    t.total = total;
    t
}

pub fn app(ctx: &Context, shared: Arc<Shared>, quit: Arc<AtomicBool>) -> Option<App> {
    let scene = std::env::var("LAN_TRANSFER_DEMO").ok()?;
    if std::env::var("LAN_TRANSFER_THEME").as_deref() == Ok("dark") {
        ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
    } else {
        ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Light);
    }
    shared.ready.store(true, Ordering::Relaxed);
    {
        let mut s = shared.settings.lock().unwrap();
        s.name = "示例 MacBook Pro".into();
        s.folder = PathBuf::from("/Users/demo/Downloads/邻传接收");
        s.trusted = vec![TrustedDevice {
            id: "01".repeat(32),
            name: "办公室电脑".into(),
            platform: Platform::Windows,
            address: "192.168.1.40".into(),
        }];
    }
    let mut app = App::new(shared.clone(), quit, None, None);
    let dir = std::env::temp_dir().join("lan-transfer-demo");
    let _ = std::fs::create_dir_all(dir.join("旅行照片/第一天"));
    for (name, size) in [
        ("季度报告 2026.pdf", 2_310_000),
        ("会议纪要.docx", 84_000),
        ("旅行照片/第一天/IMG_0001.HEIC", 3_400_000),
        ("旅行照片/第一天/IMG_0002.HEIC", 2_900_000),
        ("旅行照片/封面.jpg", 1_200_000),
    ] {
        let _ = std::fs::write(dir.join(name), vec![0u8; size]);
    }
    let peers = vec![
        peer(1, "办公室电脑", Platform::Windows, "192.168.1.40"),
        peer(2, "客厅 iMac", Platform::Mac, "192.168.1.52"),
        peer(3, "DESKTOP-7F3K2L", Platform::Windows, "192.168.1.63"),
    ];
    if scene != "empty" {
        *shared.peers.lock().unwrap() = peers;
    }
    let add_running = |shared: &Shared| {
        let mut t = transfer(
            true,
            "办公室电脑",
            &["季度报告 2026.pdf", "旅行照片"],
            4,
            9_810_000_000,
        );
        t.stage = Stage::Running;
        t.done = 5_830_000_000;
        t.files_done = 2;
        t.rate = 58_400_000.;
        t.current = "IMG_0002.HEIC".into();
        shared.add(t);
    };
    match scene.as_str() {
        "home" => {
            app.selected = Some("01".repeat(32));
            app.add_paths(vec![
                dir.join("季度报告 2026.pdf"),
                dir.join("旅行照片"),
                dir.join("会议纪要.docx"),
            ]);
        }
        "busy" => {
            app.selected = Some("01".repeat(32));
            add_running(&shared);
        }
        "text" => {
            app.selected = Some("02".repeat(32));
            app.mode = Mode::Text;
            app.text = "明天的会议链接：https://meeting.example.com/j/8842 \n密码 2026".into();
        }
        "transfers" => {
            app.page = Page::Transfers;
            let mut t = transfer(false, "客厅 iMac", &["设计稿.sketch"], 1, 48_200_000);
            t.stage = Stage::Done;
            t.saved = vec![dir.join("设计稿.sketch")];
            shared.add(t);
            let mut t = transfer(true, "DESKTOP-7F3K2L", &["安装包.dmg"], 1, 812_000_000);
            t.stage = Stage::Declined;
            t.detail = "对方拒绝了接收".into();
            t.target = Some(("192.168.1.63:45873".parse().unwrap(), String::new()));
            shared.add(t);
            let mut t = Transfer::new(false, "办公室电脑", "192.168.1.40");
            t.text = Some("https://meeting.example.com/j/8842".into());
            t.stage = Stage::Done;
            shared.add(t);
            let mut t = transfer(false, "办公室电脑", &["项目资料"], 36, 120_400_000);
            t.stage = Stage::Failed;
            t.detail = "连接已断开：对方可能已取消，或网络中断（已保存的部分可以打开）".into();
            shared.add(t);
            add_running(&shared);
            let mut t = transfer(true, "客厅 iMac", &["视频.mov"], 1, 2_400_000_000);
            t.stage = Stage::Waiting;
            shared.add(t);
        }
        "settings" => app.page = Page::Settings,
        "request" => {
            let (tx, rx) = mpsc::sync_channel(1);
            std::mem::forget(rx);
            shared.requests.lock().unwrap().push(Request {
                id: 1,
                peer: "办公室电脑".into(),
                peer_id: "01".repeat(32),
                platform: Platform::Windows,
                address: "192.168.1.40".into(),
                items: vec![
                    ItemSummary {
                        name: "季度报告 2026.pdf".into(),
                        dir: false,
                        size: 2_310_000,
                        files: 1,
                    },
                    ItemSummary {
                        name: "旅行照片".into(),
                        dir: true,
                        size: 1_120_000_000,
                        files: 128,
                    },
                    ItemSummary {
                        name: "会议纪要.docx".into(),
                        dir: false,
                        size: 84_000,
                        files: 1,
                    },
                ],
                files: 130,
                total: 1_122_394_000,
                created: Instant::now(),
                decision: tx,
            });
        }
        "message" => shared.messages.lock().unwrap().push(Message {
            id: 1,
            peer: "办公室电脑".into(),
            text:
                "明天的会议链接：https://meeting.example.com/j/8842\n密码 2026，记得带上季度报告。"
                    .into(),
        }),
        "address" => {
            app.address_dialog = AddressDialog {
                open: true,
                input: "192.168.1.".into(),
                ..Default::default()
            }
        }
        "exit" => {
            add_running(&shared);
            app.exit_confirm = true;
        }
        "toast" => {
            app.selected = Some("01".repeat(32));
            app.notify_with(
                Tone::Success,
                "已收到「办公室电脑」发来的 5 个文件",
                "打开文件夹",
                ToastAction::OpenFolder(dir.clone()),
            );
        }
        _ => {}
    }
    Some(app)
}
