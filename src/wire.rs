//! Encrypted, mutually authenticated connection between two computers.
//!
//! Each connection runs a Noise XX handshake (X25519, ChaCha20-Poly1305,
//! SHA-256): both sides prove possession of their long-term device key, and
//! every connection gets fresh session keys. After the handshake, frames are
//! a 2-byte big-endian length followed by one Noise transport message.
use crate::model::{to_hex, Identity, Platform};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

/// An error whose message is shown to the user as is.
pub fn fail(s: impl Into<String>) -> Error {
    io::Error::new(io::ErrorKind::InvalidData, s.into()).into()
}

const PATTERN: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
/// Written first by both sides, so a different program or an incompatible
/// version of this one is recognised at once instead of timing out.
const MAGIC: &[u8; 8] = b"LANT/2\r\n";
const MAX_MESSAGE: usize = 65535;
/// Largest plaintext that fits in one frame.
pub const MAX_PLAIN: usize = MAX_MESSAGE - 16;

/// Sent inside the handshake so each side learns the other's device name.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct About {
    pub name: String,
    #[serde(default)]
    pub platform: Platform,
}

/// The other side of a connection, as proven by the handshake.
#[derive(Clone, Debug)]
pub struct Remote {
    /// Public key (hex).
    pub id: String,
    pub about: About,
}

pub struct Secure {
    stream: TcpStream,
    noise: snow::TransportState,
    frame: Vec<u8>,
    plain: Vec<u8>,
}

fn builder(identity: &Identity) -> Result<snow::Builder<'_>> {
    Ok(snow::Builder::new(PATTERN.parse()?)
        .prologue(MAGIC)?
        .local_private_key(&identity.private)?)
}

fn write_frame(stream: &mut TcpStream, data: &[u8]) -> io::Result<()> {
    let mut frame = Vec::with_capacity(2 + data.len());
    frame.extend_from_slice(&(data.len() as u16).to_be_bytes());
    frame.extend_from_slice(data);
    stream.write_all(&frame)
}

fn read_frame(stream: &mut TcpStream, buf: &mut [u8]) -> Result<usize> {
    let mut len = [0; 2];
    stream.read_exact(&mut len)?;
    let len = u16::from_be_bytes(len) as usize;
    if len == 0 || len > buf.len() {
        return Err(fail("收到异常数据"));
    }
    stream.read_exact(&mut buf[..len])?;
    Ok(len)
}

fn read_magic(stream: &mut TcpStream) -> Result<()> {
    let mut magic = [0; 8];
    stream.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(Box::new(Incompatible));
    }
    Ok(())
}

/// The other side does not speak this protocol version.
#[derive(Debug)]
pub struct Incompatible;
impl std::fmt::Display for Incompatible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("对方的邻传版本与本机不同，请两台电脑都更新到最新版")
    }
}
impl std::error::Error for Incompatible {}

fn about(payload: &[u8]) -> Result<About> {
    let mut about: About =
        serde_json::from_slice(payload).map_err(|_| fail("对方发送的设备信息无效"))?;
    about.name = clean_name(&about.name);
    Ok(about)
}

/// Device names come from the network: strip control characters and limit
/// the length before they reach the interface.
pub fn clean_name(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(48)
        .collect();
    let name = name.trim();
    if name.is_empty() {
        "未命名设备".into()
    } else {
        name.into()
    }
}

/// Open a secure channel as the connecting side.
pub fn connect(mut stream: TcpStream, identity: &Identity, me: &About) -> Result<(Secure, Remote)> {
    let mut hs = builder(identity)?.build_initiator()?;
    let mut buf = vec![0; MAX_MESSAGE];
    let mut payload = vec![0; MAX_MESSAGE];
    stream.write_all(MAGIC)?;
    // -> e
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(&mut stream, &buf[..n])?;
    read_magic(&mut stream)?;
    // <- e, ee, s, es  (+ the receiver's name)
    let len = read_frame(&mut stream, &mut buf)?;
    let n = hs.read_message(&buf[..len], &mut payload)?;
    let their = about(&payload[..n])?;
    // -> s, se  (+ our name)
    let n = hs.write_message(&serde_json::to_vec(me)?, &mut buf)?;
    write_frame(&mut stream, &buf[..n])?;
    finish(stream, hs, their)
}

/// Answer a secure channel as the listening side.
pub fn accept(mut stream: TcpStream, identity: &Identity, me: &About) -> Result<(Secure, Remote)> {
    let mut hs = builder(identity)?.build_responder()?;
    let mut buf = vec![0; MAX_MESSAGE];
    let mut payload = vec![0; MAX_MESSAGE];
    read_magic(&mut stream)?;
    stream.write_all(MAGIC)?;
    // -> e
    let len = read_frame(&mut stream, &mut buf)?;
    hs.read_message(&buf[..len], &mut payload)?;
    // <- e, ee, s, es
    let n = hs.write_message(&serde_json::to_vec(me)?, &mut buf)?;
    write_frame(&mut stream, &buf[..n])?;
    // -> s, se
    let len = read_frame(&mut stream, &mut buf)?;
    let n = hs.read_message(&buf[..len], &mut payload)?;
    let their = about(&payload[..n])?;
    finish(stream, hs, their)
}

fn finish(stream: TcpStream, hs: snow::HandshakeState, about: About) -> Result<(Secure, Remote)> {
    let id = to_hex(
        hs.get_remote_static()
            .ok_or_else(|| fail("对方没有提供设备身份"))?,
    );
    let noise = hs.into_transport_mode()?;
    Ok((
        Secure {
            stream,
            noise,
            frame: vec![0; 2 + MAX_MESSAGE],
            plain: vec![0; MAX_MESSAGE],
        },
        Remote { id, about },
    ))
}

impl Secure {
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    /// Send one frame of at most [`MAX_PLAIN`] bytes.
    pub fn send(&mut self, plain: &[u8]) -> Result<()> {
        if plain.len() > MAX_PLAIN {
            return Err(fail("数据帧过大"));
        }
        let n = self.noise.write_message(plain, &mut self.frame[2..])?;
        self.frame[..2].copy_from_slice(&(n as u16).to_be_bytes());
        self.stream.write_all(&self.frame[..2 + n])?;
        Ok(())
    }

    /// Receive one frame. The data is valid until the next call.
    pub fn recv(&mut self) -> Result<&[u8]> {
        let len = read_frame(&mut self.stream, &mut self.frame)?;
        let n = self
            .noise
            .read_message(&self.frame[..len], &mut self.plain)?;
        Ok(&self.plain[..n])
    }

    /// Send a JSON value, split across frames when it is large (a request
    /// for a folder with thousands of files).
    pub fn send_json<T: Serialize>(&mut self, value: &T) -> Result<()> {
        let data = serde_json::to_vec(value)?;
        let first = data.len().min(MAX_PLAIN - 4);
        let mut head = Vec::with_capacity(4 + first);
        head.extend_from_slice(&(data.len() as u32).to_be_bytes());
        head.extend_from_slice(&data[..first]);
        self.send(&head)?;
        for part in data[first..].chunks(MAX_PLAIN) {
            self.send(part)?;
        }
        Ok(())
    }

    pub fn recv_json<T: DeserializeOwned>(&mut self, limit: usize) -> Result<T> {
        let head = self.recv()?;
        if head.len() < 4 {
            return Err(fail("收到异常数据"));
        }
        let total = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        if total > limit {
            return Err(fail("请求内容过大"));
        }
        let mut data = head[4..].to_vec();
        while data.len() < total {
            let part = self.recv()?;
            data.extend_from_slice(part);
        }
        if data.len() != total {
            return Err(fail("收到异常数据"));
        }
        serde_json::from_slice(&data).map_err(|_| fail("收到无法识别的请求"))
    }
}
