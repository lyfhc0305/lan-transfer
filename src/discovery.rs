//! Finding other computers. Every few seconds each copy broadcasts a small
//! "hello" on the local network and the others answer, so devices appear and
//! disappear by themselves. A "bye" removes a device at once when it quits or
//! stops receiving.
use crate::{model::*, network::local_addresses, wire::clean_name};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

const PROTOCOL: &str = "lan-transfer/2";
const INTERVAL: Duration = Duration::from_secs(3);
/// A device that has not been heard from for this long is removed.
const EXPIRY: Duration = Duration::from_secs(10);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Hello,
    Reply,
    Bye,
}

#[derive(Serialize, Deserialize, Debug)]
struct Beacon {
    p: String,
    kind: Kind,
    id: String,
    name: String,
    #[serde(default)]
    platform: Platform,
    port: u16,
    /// False while this computer does not accept files: others do not list
    /// it, but still answer so it can find them.
    receiving: bool,
}

pub fn start(shared: Arc<Shared>) {
    let socket = match UdpSocket::bind(("0.0.0.0", PORT)) {
        Ok(s) => s,
        Err(e) => {
            // Without the shared port others cannot find this computer, but
            // it can still find them (their answers come back to any port).
            *shared.discovery_note.lock().unwrap() = Some(format!(
                "其他电脑无法自动发现本机（UDP 端口 {PORT}：{e}），可在对方输入本机 IP 连接"
            ));
            match UdpSocket::bind(("0.0.0.0", 0)) {
                Ok(s) => s,
                Err(_) => return,
            }
        }
    };
    let _ = socket.set_broadcast(true);
    let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));
    *shared.beacon.lock().unwrap() = socket.try_clone().ok();
    let Ok(listener) = socket.try_clone() else {
        return;
    };
    let s = shared.clone();
    std::thread::spawn(move || listen(listener, s));
    std::thread::spawn(move || loop {
        send(&socket, &shared, Kind::Hello, &targets(&shared));
        prune(&shared);
        std::thread::sleep(INTERVAL);
    });
}

/// Announce now, e.g. after receiving was switched on or the name changed.
pub fn announce(shared: &Shared) {
    if let Some(socket) = shared.beacon.lock().unwrap().as_ref() {
        send(socket, shared, Kind::Hello, &targets(shared));
    }
}

/// Tell the others to remove this computer (quitting, receiving switched off).
pub fn goodbye(shared: &Shared) {
    if let Some(socket) = shared.beacon.lock().unwrap().as_ref() {
        send(socket, shared, Kind::Bye, &targets(shared));
    }
}

/// Ask a single address, typed in by the user, to answer.
pub fn probe(shared: &Shared, ip: IpAddr) {
    {
        let mut manual = shared.manual.lock().unwrap();
        if !manual.contains(&ip) {
            manual.push(ip);
        }
    }
    if let Some(socket) = shared.beacon.lock().unwrap().as_ref() {
        send(socket, shared, Kind::Hello, &[SocketAddr::new(ip, PORT)]);
    }
}

fn beacon(shared: &Shared, kind: Kind) -> Beacon {
    let s = shared.settings.lock().unwrap();
    Beacon {
        p: PROTOCOL.into(),
        kind,
        id: shared.id(),
        name: s.name.clone(),
        platform: Platform::this(),
        port: PORT,
        receiving: s.receive && kind != Kind::Bye,
    }
}

fn send(socket: &UdpSocket, shared: &Shared, kind: Kind, to: &[SocketAddr]) {
    let Ok(data) = serde_json::to_vec(&beacon(shared, kind)) else {
        return;
    };
    for addr in to {
        let _ = socket.send_to(&data, addr);
    }
}

/// Broadcast addresses of every network, plus devices known by address
/// (trusted devices and addresses typed in), for networks that drop
/// broadcasts.
fn targets(shared: &Shared) -> Vec<SocketAddr> {
    let mut ips: HashSet<IpAddr> = HashSet::new();
    for x in if_addrs::get_if_addrs().unwrap_or_default() {
        if let if_addrs::IfAddr::V4(v) = x.addr {
            if !v.ip.is_loopback() {
                if let Some(b) = v.broadcast {
                    ips.insert(IpAddr::V4(b));
                }
            }
        }
    }
    ips.insert(IpAddr::V4(Ipv4Addr::BROADCAST));
    for t in &shared.settings.lock().unwrap().trusted {
        if let Ok(ip) = t.address.parse() {
            ips.insert(ip);
        }
    }
    ips.extend(shared.manual.lock().unwrap().iter().copied());
    // Never probe ourselves by address.
    for own in local_addresses() {
        ips.remove(&IpAddr::V4(own));
    }
    ips.into_iter()
        .map(|ip| SocketAddr::new(ip, PORT))
        .collect()
}

fn listen(socket: UdpSocket, shared: Arc<Shared>) {
    let mut buf = [0; 2048];
    loop {
        let (n, from) = match socket.recv_from(&mut buf) {
            Ok(v) => v,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                        // Windows reports an ICMP "port unreachable" caused by an
                        // earlier datagram as a reset on the next receive.
                        | io::ErrorKind::ConnectionReset
                ) =>
            {
                continue
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
        };
        let Ok(b) = serde_json::from_slice::<Beacon>(&buf[..n]) else {
            continue;
        };
        if b.p != PROTOCOL
            || b.id.len() != 64
            || !b.id.bytes().all(|c| c.is_ascii_hexdigit())
            || b.id == shared.id()
        {
            continue;
        }
        match b.kind {
            Kind::Bye => remove(&shared, &b.id),
            Kind::Hello | Kind::Reply => {
                if b.receiving {
                    upsert(&shared, &b, from.ip());
                } else {
                    remove(&shared, &b.id);
                }
                if b.kind == Kind::Hello && shared.settings.lock().unwrap().receive {
                    send(&socket, &shared, Kind::Reply, &[from]);
                }
            }
        }
    }
}

fn upsert(shared: &Shared, b: &Beacon, ip: IpAddr) {
    let address = SocketAddr::new(ip, b.port);
    let name = clean_name(&b.name);
    let mut peers = shared.peers.lock().unwrap();
    let changed = match peers.iter_mut().find(|p| p.id == b.id) {
        Some(p) => {
            let changed = p.name != name || p.address != address || p.platform != b.platform;
            p.name = name;
            p.address = address;
            p.platform = b.platform;
            p.seen = Instant::now();
            changed
        }
        None => {
            peers.push(Peer {
                id: b.id.clone(),
                name,
                platform: b.platform,
                address,
                seen: Instant::now(),
            });
            true
        }
    };
    drop(peers);
    if changed {
        shared.wake();
    }
}

fn remove(shared: &Shared, id: &str) {
    let mut peers = shared.peers.lock().unwrap();
    let before = peers.len();
    peers.retain(|p| p.id != id);
    if peers.len() != before {
        drop(peers);
        shared.wake();
    }
}

fn prune(shared: &Shared) {
    let mut peers = shared.peers.lock().unwrap();
    let before = peers.len();
    peers.retain(|p| p.seen.elapsed() < EXPIRY);
    if peers.len() != before {
        drop(peers);
        shared.wake();
    }
}

#[cfg(test)]
pub fn test_listen(socket: UdpSocket, shared: Arc<Shared>) {
    std::thread::spawn(move || listen(socket, shared));
}

#[cfg(test)]
pub fn test_send(socket: &UdpSocket, shared: &Shared, kind: &str, to: SocketAddr) {
    let kind = match kind {
        "hello" => Kind::Hello,
        "bye" => Kind::Bye,
        _ => Kind::Reply,
    };
    send(socket, shared, kind, &[to]);
}
