use super::*;
use std::{net::UdpSocket, sync::Once, thread};

/// Settings writes (trusting a device) go to a throw-away directory.
fn isolate_config() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("lan-transfer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LAN_TRANSFER_CONFIG_DIR", dir);
    });
}

fn shared(folder: &Path) -> Arc<Shared> {
    isolate_config();
    Shared::new(
        Settings {
            folder: folder.into(),
            name: "测试电脑".into(),
            ..Settings::default()
        },
        eframe::egui::Context::default(),
    )
}

/// A receiver that trusts `sender`, so nothing needs to be confirmed.
fn trusting(folder: &Path, sender: &Shared) -> Arc<Shared> {
    let s = shared(folder);
    s.settings.lock().unwrap().trusted.push(TrustedDevice {
        id: sender.id(),
        name: "发送方".into(),
        platform: Platform::Mac,
        address: String::new(),
    });
    s
}

fn one_server(s: Arc<Shared>) -> (SocketAddr, thread::JoinHandle<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || handle(listener.accept().unwrap().0, &s).is_ok());
    (addr, handle)
}

fn wait_for(mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        thread::sleep(Duration::from_millis(5));
    }
}

fn target(addr: SocketAddr) -> Target {
    Target {
        address: addr,
        id: String::new(),
        name: "对方".into(),
    }
}

fn start(s: &Arc<Shared>, addr: SocketAddr, payload: Payload) -> u64 {
    send(s.clone(), target(addr), payload)
}

fn finish(s: &Shared, id: u64) -> Transfer {
    wait_for(|| s.transfer(id).unwrap().stage.finished());
    s.transfer(id).unwrap()
}

fn send_now(s: &Arc<Shared>, addr: SocketAddr, payload: Payload) -> Transfer {
    let id = start(s, addr, payload);
    finish(s, id)
}

fn files(paths: &[&Path]) -> Payload {
    Payload::Files(paths.iter().map(|p| p.to_path_buf()).collect())
}

fn data(size: usize) -> Vec<u8> {
    (0..size).map(|x| (x % 251) as u8).collect()
}

fn tree(dir: &Path) -> Vec<String> {
    let mut out = vec![];
    fn go(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let rel = p
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if p.is_dir() {
                out.push(format!("{rel}/"));
                go(base, &p, out);
            } else {
                out.push(rel);
            }
        }
    }
    if dir.exists() {
        go(dir, dir, &mut out);
    }
    out.sort();
    out
}

#[test]
fn files_and_folders_arrive_intact_in_one_batch() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("相册/2024")).unwrap();
    fs::create_dir_all(src.join("相册/空文件夹")).unwrap();
    fs::write(src.join("相册/2024/a.jpg"), data(70_000)).unwrap();
    fs::write(src.join("相册/.DS_Store"), b"junk").unwrap();
    fs::write(src.join("中文 文件.txt"), data(1234)).unwrap();
    fs::write(src.join("empty.bin"), b"").unwrap();
    fs::write(src.join("large.bin"), data(9 * 1024 * 1024 + 7)).unwrap();
    let dest = dir.path().join("received");
    let sender = shared(&src);
    let receiver = trusting(&dest, &sender);
    let (addr, h) = one_server(receiver.clone());
    let t = send_now(
        &sender,
        addr,
        files(&[
            &src.join("中文 文件.txt"),
            &src.join("empty.bin"),
            &src.join("large.bin"),
            &src.join("相册"),
        ]),
    );
    assert_eq!(t.stage, Stage::Done, "{}", t.detail);
    assert!(h.join().unwrap());
    assert_eq!((t.files, t.files_done), (4, 4));
    assert_eq!(t.items.len(), 4);
    assert!(
        receiver.requests.lock().unwrap().is_empty(),
        "trusted: no prompt"
    );
    assert_eq!(
        tree(&dest),
        [
            "empty.bin",
            "large.bin",
            "中文 文件.txt",
            "相册/",
            "相册/2024/",
            "相册/2024/a.jpg",
            "相册/空文件夹/"
        ]
    );
    for name in ["中文 文件.txt", "empty.bin", "large.bin", "相册/2024/a.jpg"] {
        assert_eq!(
            fs::read(dest.join(name)).unwrap(),
            fs::read(src.join(name)).unwrap()
        );
    }
    let r = receiver.transfers.lock().unwrap()[0].clone();
    assert_eq!(r.stage, Stage::Done);
    assert_eq!(r.saved.len(), 4);
    assert_eq!(r.peer, "测试电脑");
}

#[test]
fn untrusted_sender_is_asked_once_and_can_be_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("recv");
    let names = ["a.txt", "b.txt", "c.txt"];
    for n in names {
        fs::write(dir.path().join(n), n.as_bytes()).unwrap();
    }
    let paths: Vec<PathBuf> = names.iter().map(|n| dir.path().join(n)).collect();
    let sender = shared(dir.path());
    let receiver = shared(&dest);
    let (addr, h) = one_server(receiver.clone());
    let id = start(&sender, addr, Payload::Files(paths.clone()));
    wait_for(|| !receiver.requests.lock().unwrap().is_empty());
    {
        let requests = receiver.requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "one request for the whole batch");
        let r = &requests[0];
        assert_eq!((r.files, r.total, r.items.len()), (3, 15, 3));
        assert_eq!(r.peer_id, sender.id());
        assert_eq!(sender.transfer(id).unwrap().stage, Stage::Waiting);
        r.decision
            .send(Answer {
                accept: true,
                trust: true,
            })
            .unwrap();
    }
    assert_eq!(finish(&sender, id).stage, Stage::Done);
    assert!(h.join().unwrap());
    assert!(receiver.settings.lock().unwrap().is_trusted(&sender.id()));
    // Trusted now: the next batch arrives without a prompt, renamed.
    let (addr, h) = one_server(receiver.clone());
    let t = send_now(&sender, addr, Payload::Files(paths));
    assert_eq!(t.stage, Stage::Done, "{}", t.detail);
    assert!(h.join().unwrap());
    assert_eq!(
        tree(&dest),
        [
            "a (2).txt",
            "a.txt",
            "b (2).txt",
            "b.txt",
            "c (2).txt",
            "c.txt"
        ]
    );
}

#[test]
fn declining_stops_the_whole_batch() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("recv");
    fs::write(dir.path().join("x.txt"), b"x").unwrap();
    fs::write(dir.path().join("y.txt"), b"y").unwrap();
    let sender = shared(dir.path());
    let receiver = shared(&dest);
    let (addr, h) = one_server(receiver.clone());
    let id = start(
        &sender,
        addr,
        files(&[&dir.path().join("x.txt"), &dir.path().join("y.txt")]),
    );
    wait_for(|| !receiver.requests.lock().unwrap().is_empty());
    receiver.requests.lock().unwrap()[0]
        .decision
        .send(Answer {
            accept: false,
            trust: false,
        })
        .unwrap();
    let t = finish(&sender, id);
    assert_eq!(t.stage, Stage::Declined);
    assert!(t.detail.contains("拒绝"), "{}", t.detail);
    assert!(!h.join().unwrap());
    assert!(tree(&dest).is_empty());
    let r = receiver.transfers.lock().unwrap()[0].clone();
    assert_eq!((r.stage, r.detail.as_str()), (Stage::Declined, "已拒绝"));
    assert!(!receiver.settings.lock().unwrap().is_trusted(&sender.id()));
}

#[test]
fn existing_names_are_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("recv");
    fs::create_dir_all(dest.join("资料")).unwrap();
    fs::write(dest.join("file.txt"), b"original").unwrap();
    fs::write(dest.join("资料/old.txt"), b"old").unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(src.join("资料")).unwrap();
    fs::write(src.join("file.txt"), b"new").unwrap();
    fs::write(src.join("资料/new.txt"), b"new").unwrap();
    let sender = shared(&src);
    let receiver = trusting(&dest, &sender);
    let (addr, h) = one_server(receiver);
    let t = send_now(
        &sender,
        addr,
        files(&[&src.join("file.txt"), &src.join("资料")]),
    );
    assert_eq!(t.stage, Stage::Done, "{}", t.detail);
    assert!(h.join().unwrap());
    assert_eq!(
        tree(&dest),
        [
            "file (2).txt",
            "file.txt",
            "资料 (2)/",
            "资料 (2)/new.txt",
            "资料/",
            "资料/old.txt"
        ]
    );
    assert_eq!(fs::read(dest.join("file.txt")).unwrap(), b"original");
}

#[test]
fn text_is_delivered_without_files() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("recv");
    let sender = shared(dir.path());
    let receiver = shared(&dest);
    let (addr, h) = one_server(receiver.clone());
    let text = "会议链接 https://example.com/room?id=42\n第二行";
    let t = send_now(&sender, addr, Payload::Text(text.into()));
    assert_eq!(t.stage, Stage::Done, "{}", t.detail);
    assert!(h.join().unwrap());
    let messages = receiver.messages.lock().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, text);
    assert_eq!(messages[0].peer, "测试电脑");
    assert!(!dest.exists());
    // Empty text never leaves the sender.
    let (addr, _h) = one_server(receiver.clone());
    let t = send_now(&sender, addr, Payload::Text("  \n ".into()));
    assert_eq!(t.stage, Stage::Failed);
}

#[test]
fn unread_texts_are_limited() {
    let dir = tempfile::tempdir().unwrap();
    let receiver = shared(&dir.path().join("recv"));
    let sender = shared(dir.path());
    for i in 0..8 {
        let (addr, h) = one_server(receiver.clone());
        let t = send_now(&sender, addr, Payload::Text(format!("第 {i} 条")));
        assert_eq!(t.stage, Stage::Done, "{}", t.detail);
        assert!(h.join().unwrap());
        // The message is stored after the reply goes out; do not race it.
        wait_for(|| receiver.messages.lock().unwrap().len() == i + 1);
    }
    assert_eq!(receiver.messages.lock().unwrap().len(), 8);
    // The ninth arrives while none has been read: declined with a reason.
    let (addr, h) = one_server(receiver.clone());
    let t = send_now(&sender, addr, Payload::Text("再来一条".into()));
    assert_eq!(t.stage, Stage::Declined);
    assert!(t.detail.contains("未读"), "{}", t.detail);
    // Like other declined texts, the handler ends with an error.
    assert!(!h.join().unwrap());
}

#[test]
fn paused_or_trusted_only_receivers_decline_with_a_reason() {
    for (paused, reason) in [(true, "关闭接收"), (false, "已信任")] {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("recv");
        let receiver = shared(&dest);
        {
            let mut s = receiver.settings.lock().unwrap();
            if paused {
                s.receive = false;
            } else {
                s.trusted_only = true;
            }
        }
        fs::write(dir.path().join("f"), b"secret").unwrap();
        let (addr, h) = one_server(receiver.clone());
        let t = send_now(&shared(dir.path()), addr, files(&[&dir.path().join("f")]));
        assert_eq!(t.stage, Stage::Declined);
        assert!(t.detail.contains(reason), "{}", t.detail);
        assert!(h.join().unwrap());
        assert!(!dest.exists());
        assert!(receiver.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn sender_cancel_while_waiting_withdraws_the_request() {
    let dir = tempfile::tempdir().unwrap();
    let receiver = shared(&dir.path().join("recv"));
    let sender = shared(dir.path());
    fs::write(dir.path().join("file"), b"data").unwrap();
    let (addr, h) = one_server(receiver.clone());
    let id = start(&sender, addr, files(&[&dir.path().join("file")]));
    wait_for(|| !receiver.requests.lock().unwrap().is_empty());
    let started = Instant::now();
    sender
        .transfer(id)
        .unwrap()
        .cancel
        .store(true, Ordering::Relaxed);
    assert_eq!(finish(&sender, id).stage, Stage::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(2));
    // The receiver notices and withdraws its dialog instead of leaving it
    // on screen until the timeout.
    wait_for(|| receiver.requests.lock().unwrap().is_empty());
    assert!(!h.join().unwrap());
    let r = receiver.transfers.lock().unwrap()[0].clone();
    assert_eq!(r.stage, Stage::Cancelled);
    assert!(r.detail.contains("对方已取消"), "{}", r.detail);
}

#[test]
fn receiver_cancel_stops_the_sender_and_removes_partial_files() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("recv");
    let src = dir.path().join("big.bin");
    fs::write(&src, data(64 * 1024 * 1024)).unwrap();
    let sender = shared(dir.path());
    let receiver = trusting(&dest, &sender);
    let (addr, h) = one_server(receiver.clone());
    let id = start(&sender, addr, files(&[&src]));
    wait_for(|| {
        receiver
            .transfers
            .lock()
            .unwrap()
            .first()
            .is_some_and(|t| t.stage == Stage::Running)
    });
    receiver.transfers.lock().unwrap()[0]
        .cancel
        .store(true, Ordering::Relaxed);
    assert!(!h.join().unwrap());
    let t = finish(&sender, id);
    assert_eq!(t.stage, Stage::Failed);
    assert_eq!(
        receiver.transfers.lock().unwrap()[0].stage,
        Stage::Cancelled
    );
    assert!(tree(&dest).is_empty(), "{:?}", tree(&dest));
}

#[test]
fn a_changed_identity_stops_sending() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("f"), b"x").unwrap();
    let sender = shared(dir.path());
    let receiver = trusting(&dir.path().join("recv"), &sender);
    let (addr, _h) = one_server(receiver);
    let mut t = target(addr);
    t.id = Identity::generate().id();
    let id = send(sender.clone(), t, files(&[&dir.path().join("f")]));
    let t = finish(&sender, id);
    assert_eq!(t.stage, Stage::Failed);
    assert!(t.detail.contains("设备身份"), "{}", t.detail);
}

#[test]
fn another_program_on_the_port_is_reported_as_incompatible() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("f"), b"x").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let _ = s.write_all(&[7; 32]);
        thread::sleep(Duration::from_millis(500));
    });
    let t = send_now(&shared(dir.path()), addr, files(&[&dir.path().join("f")]));
    assert_eq!(t.stage, Stage::Failed);
    assert!(t.detail.contains("版本"), "{}", t.detail);
}

/// Connect like a sender and hand over the raw offer.
fn raw_client(addr: SocketAddr, entries: Vec<Entry>) -> Secure {
    let stream = TcpStream::connect(addr).unwrap();
    configure(&stream, Duration::from_secs(5)).unwrap();
    let me = About {
        name: "攻击者".into(),
        platform: Platform::Other,
    };
    let (mut ch, _) = wire::connect(stream, &Identity::generate(), &me).unwrap();
    ch.send_json(&Offer::Files { entries }).unwrap();
    ch
}

fn file(path: &str, size: u64) -> Entry {
    Entry {
        path: path.into(),
        size,
        dir: false,
    }
}

#[test]
fn unsafe_requests_are_refused_before_touching_the_disk() {
    let cases: Vec<Vec<Entry>> = vec![
        vec![file("../escape.txt", 1)],
        vec![file("a/../../escape.txt", 1)],
        vec![file("/etc/passwd", 1)],
        vec![file("C:evil.txt", 1)],
        vec![file("folder\\evil.txt", 1)],
        vec![file("CON.txt", 1)],
        vec![file("safe.txt\u{202e}gpj.exe", 1)],
        vec![file("a.txt", 1), file("A.TXT", 1)],
        vec![file("a", 1), file("a/b.txt", 1)],
        vec![file("huge.bin", 101 * 1024 * 1024 * 1024)],
        vec![],
    ];
    for entries in cases {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("recv");
        let (addr, h) = one_server(shared(&dest));
        let shown = format!("{entries:?}");
        let mut ch = raw_client(addr, entries);
        let reply: Reply = ch.recv_json(4096).unwrap();
        assert!(!reply.accept, "{shown}");
        assert!(!h.join().unwrap(), "{shown}");
        assert!(!dest.exists(), "{shown}");
        assert!(!dir.path().join("escape.txt").exists());
    }
}

#[test]
fn the_sender_is_told_why_a_transfer_failed() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("recv");
    let receiver = shared(&dest);
    let (addr, h) = one_server(receiver.clone());
    let mut ch = raw_client(addr, vec![file("f.bin", 4)]);
    wait_for(|| !receiver.requests.lock().unwrap().is_empty());
    receiver.requests.lock().unwrap()[0]
        .decision
        .send(Answer {
            accept: true,
            trust: false,
        })
        .unwrap();
    let reply: Reply = ch.recv_json(4096).unwrap();
    assert!(reply.accept);
    ch.send(b"data").unwrap();
    ch.send(&[9; 32]).unwrap();
    let receipt: Receipt = ch.recv_json(4096).unwrap();
    assert!(!receipt.ok, "the failure is reported, not a bare drop");
    assert!(receipt.reason.contains("校验"), "{}", receipt.reason);
    drop(ch);
    assert!(!h.join().unwrap());
    assert!(tree(&dest).is_empty());
}

#[test]
fn broken_uploads_leave_no_partial_files() {
    for mode in ["partial", "hash", "long", "cancel", "garbage"] {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("recv");
        let receiver = shared(&dest);
        let (addr, h) = one_server(receiver.clone());
        let mut ch = raw_client(addr, vec![file("f.bin", 4)]);
        wait_for(|| !receiver.requests.lock().unwrap().is_empty());
        receiver.requests.lock().unwrap()[0]
            .decision
            .send(Answer {
                accept: true,
                trust: false,
            })
            .unwrap();
        let reply: Reply = ch.recv_json(4096).unwrap();
        assert!(reply.accept);
        match mode {
            "partial" => ch.send(b"ab").unwrap(),
            "hash" => {
                ch.send(b"data").unwrap();
                ch.send(&[0; 32]).unwrap();
            }
            "long" => ch.send(b"too long").unwrap(),
            "cancel" => {
                ch.send(b"da").unwrap();
                wait_for(|| receiver.transfers.lock().unwrap()[0].stage == Stage::Running);
                receiver.transfers.lock().unwrap()[0]
                    .cancel
                    .store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(300));
            }
            "garbage" => {
                ch.stream()
                    .try_clone()
                    .unwrap()
                    .write_all(&[0, 40])
                    .unwrap();
                ch.stream()
                    .try_clone()
                    .unwrap()
                    .write_all(&[1; 40])
                    .unwrap();
            }
            _ => unreachable!(),
        }
        drop(ch);
        assert!(!h.join().unwrap(), "{mode}");
        assert!(
            tree(&dest).is_empty(),
            "left a partial file: {mode}: {:?}",
            tree(&dest)
        );
        let r = receiver.transfers.lock().unwrap()[0].clone();
        assert!(r.stage.finished() && r.stage != Stage::Done, "{mode}");
    }
}

#[test]
fn folders_skip_links_and_system_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("项目");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();
    fs::write(root.join(".gitignore"), b"target").unwrap();
    fs::write(root.join("Thumbs.db"), b"").unwrap();
    fs::write(root.join("._main.rs"), b"").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path(), root.join("loop")).unwrap();
    #[cfg(unix)]
    fs::write(root.join("a:b.txt"), b"").unwrap();
    let (list, skipped) = scan(std::slice::from_ref(&root), &AtomicBool::new(false)).unwrap();
    let paths: Vec<_> = list.iter().map(|s| s.entry.path.as_str()).collect();
    assert_eq!(
        paths,
        ["项目", "项目/.gitignore", "项目/src", "项目/src/main.rs"]
    );
    assert_eq!(skipped, if cfg!(unix) { 2 } else { 0 });
    // Two picked items with the same name are both sent.
    let other = dir.path().join("other");
    fs::create_dir_all(other.join("项目")).unwrap();
    let (list, _) = scan(&[root, other.join("项目")], &AtomicBool::new(false)).unwrap();
    assert!(list.iter().any(|s| s.entry.path == "项目 (2)"));
    // A link picked on its own is skipped as well, not sent as a folder.
    #[cfg(unix)]
    {
        let link = dir.path().join("入口");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        let e = scan(std::slice::from_ref(&link), &AtomicBool::new(false))
            .unwrap_err()
            .to_string();
        assert_eq!(e, "没有可以发送的文件");
    }
}

#[test]
fn one_address_cannot_hold_every_connection() {
    let mut per_ip: HashMap<IpAddr, usize> = HashMap::new();
    let a: IpAddr = "192.168.1.10".parse().unwrap();
    let b: IpAddr = "192.168.1.11".parse().unwrap();
    for _ in 0..3 {
        assert!(admit(&mut per_ip, a, 3));
    }
    assert!(!admit(&mut per_ip, a, 3), "same address is refused");
    assert!(admit(&mut per_ip, b, 3), "other addresses are unaffected");
    release(&mut per_ip, a);
    assert!(admit(&mut per_ip, a, 3), "a slot frees up after a release");
    release(&mut per_ip, b);
    release(&mut per_ip, b);
    release(&mut per_ip, b);
    release(&mut per_ip, b);
    assert_eq!(per_ip.get(&b), None, "the entry is dropped at zero");
}

#[test]
fn names_and_summaries() {
    assert_eq!(numbered("报告.pdf", 2, false), "报告 (2).pdf");
    assert_eq!(numbered("照片", 3, true), "照片 (3)");
    assert_eq!(numbered(".bashrc", 2, false), ".bashrc (2)");
    assert_eq!(numbered("v1.0", 2, true), "v1.0 (2)");
    let items = summarize(&[
        Entry {
            path: "相册".into(),
            size: 0,
            dir: true,
        },
        file("相册/a.jpg", 10),
        file("相册/b/c.jpg", 5),
        file("readme.md", 3),
    ]);
    assert_eq!(items.len(), 2);
    assert!(items[0].dir && items[0].size == 15 && items[0].files == 2);
    assert!(!items[1].dir && items[1].size == 3 && items[1].files == 1);
    assert!(safe_name("正常 文件.txt") && !safe_name("aux") && !safe_name("com1.txt"));
    assert!(!safe_name("..") && !safe_name("x.") && !safe_name("a|b"));
}

#[test]
fn old_settings_get_a_new_identity_and_keep_the_rest() {
    let old = r#"{"name":"客厅电脑","id":"aa","key":"07070707","folder":"/tmp/x","receive":false,"auto_accept":true,"close_to_tray":false}"#;
    let mut s: Settings = serde_json::from_str(old).unwrap();
    s.complete();
    assert_eq!(s.name, "客厅电脑");
    assert!(!s.receive && !s.close_to_tray);
    assert!(Identity::from_hex(&s.identity).is_some());
    assert!(s.trusted.is_empty());
    let a = Identity::generate();
    let b = Identity::from_hex(&a.private_hex()).unwrap();
    assert_eq!(a.id(), b.id());
    assert_eq!(fingerprint(&a.id()).chars().count(), 14);
}

#[test]
fn discovery_finds_devices_and_forgets_them_after_bye() {
    isolate_config();
    let a = shared(Path::new("a"));
    let b = shared(Path::new("b"));
    b.settings.lock().unwrap().name = "设备 B".into();
    let sock_a = UdpSocket::bind("127.0.0.1:0").unwrap();
    let sock_b = UdpSocket::bind("127.0.0.1:0").unwrap();
    for s in [&sock_a, &sock_b] {
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
    }
    let addr_a = sock_a.local_addr().unwrap();
    let addr_b = sock_b.local_addr().unwrap();
    crate::discovery::test_listen(sock_a.try_clone().unwrap(), a.clone());
    crate::discovery::test_listen(sock_b.try_clone().unwrap(), b.clone());
    // B says hello to A; A lists B and answers, so B lists A too.
    crate::discovery::test_send(&sock_b, &b, "hello", addr_a);
    wait_for(|| a.peers.lock().unwrap().len() == 1 && b.peers.lock().unwrap().len() == 1);
    let pb = a.peers.lock().unwrap()[0].clone();
    assert_eq!(
        (pb.id.as_str(), pb.name.as_str()),
        (b.id().as_str(), "设备 B")
    );
    assert_eq!(pb.address, SocketAddr::new(addr_b.ip(), PORT));
    crate::discovery::test_send(&sock_b, &b, "bye", addr_a);
    wait_for(|| a.peers.lock().unwrap().is_empty());
    // A computer that does not receive is not listed.
    b.settings.lock().unwrap().receive = false;
    crate::discovery::test_send(&sock_b, &b, "hello", addr_a);
    thread::sleep(Duration::from_millis(300));
    assert!(a.peers.lock().unwrap().is_empty());
}
