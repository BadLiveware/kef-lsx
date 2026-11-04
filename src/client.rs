//! TCP client for the control server on port 50001.
//!
//! # The one rule
//!
//! **Hold a single connection open.** The server is effectively
//! single-session: if you connect, send one command, close, and immediately
//! reconnect, the second connect gets `ECONNREFUSED` until the server releases
//! the previous session. This is not a firewall or a rate limit — the public
//! `kefctl` client keeps one socket in a `state` variable for exactly this
//! reason.
//!
//! [`Client`] therefore owns one `TcpStream` for its whole lifetime. Do not
//! build a client per command.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};

use crate::proto::{self, Hex, Reply};

/// A connection to a speaker's control server.
pub struct Client {
    stream: TcpStream,
    addr: SocketAddr,
}

impl Client {
    /// Connect, retrying while the server is still releasing a previous session.
    pub fn connect(host: &str, timeout: Duration) -> Result<Self> {
        let addr = resolve(host)?;
        let deadline = Instant::now() + timeout;
        let mut backoff = Duration::from_millis(50);
        loop {
            match TcpStream::connect_timeout(&addr, timeout.min(Duration::from_secs(3))) {
                Ok(stream) => {
                    stream
                        .set_nodelay(true)
                        .context("set TCP_NODELAY on control socket")?;
                    stream
                        .set_read_timeout(Some(timeout))
                        .context("set read timeout")?;
                    return Ok(Client { stream, addr });
                }
                Err(e) if is_transient(&e) && Instant::now() < deadline => {
                    std::thread::sleep(backoff);
                    backoff = (backoff * 2).min(Duration::from_millis(500));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!(
                            "connect to {addr}:{} (if the speaker was just used by \
                             another client, wait a few seconds)",
                            proto::PORT
                        )
                    })
                }
            }
        }
    }

    /// Remote address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Send a raw request and read one reply.
    pub fn raw(&mut self, req: &[u8]) -> Result<Reply> {
        self.stream
            .write_all(req)
            .context("write request to control socket")?;
        self.read_reply()
    }

    /// Read a `GET` for `reg`.
    pub fn get(&mut self, reg: u8) -> Result<Reply> {
        let req = proto::request(proto::OP_GET, reg, &[], false);
        self.raw(&req)
    }

    /// Send a `SET` for `reg` with `payload`.
    ///
    /// A successful write is acknowledged with `52 11 ff` ([`Reply::Ack`]).
    /// The speaker does not echo the new value, so callers that care should
    /// read the register back — see [`Client::set_verified`].
    ///
    /// `with_crc` appends a CRC8 to the request. Unverified against hardware;
    /// see [`proto::request`].
    pub fn set(&mut self, reg: u8, payload: &[u8], with_crc: bool) -> Result<Reply> {
        let req = proto::request(proto::OP_SET, reg, payload, with_crc);
        self.raw(&req)
    }

    /// Read one reply.
    ///
    /// Completion is decided by the self-describing frame length when there is
    /// one, and otherwise by the socket going idle. The idle fallback matters:
    /// headerless chunk replies have no length at all, and a truncated frame
    /// should be reported rather than block until the caller's long timeout.
    fn read_reply(&mut self) -> Result<Reply> {
        let mut buf = Vec::with_capacity(1024);
        let mut chunk = [0u8; 1024];

        loop {
            if let Some(total) = proto::framed_len(&buf) {
                if buf.len() >= total {
                    return Ok(proto::decode(&buf[..total]));
                }
            } else if buf.len() >= 3 {
                // Headerless data (chunked blob): take what we have.
                return Ok(proto::decode(&buf));
            }

            match self.stream.read(&mut chunk) {
                Ok(0) => bail!("speaker closed the control connection"),
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
                {
                    if buf.len() >= 3 {
                        return Ok(proto::decode(&buf));
                    }
                    bail!("no reply from speaker ({} bytes received)", buf.len());
                }
                Err(e) => return Err(e).context("read from control socket"),
            }
        }
    }

    /// Read until `max` bytes or an idle timeout, for blob registers.
    ///
    /// Large reads are delivered as headerless 256-byte chunks with no
    /// terminator, so the only way to know a blob ended is to stop seeing
    /// data. Set the read timeout on the client accordingly.
    pub fn read_blob(&mut self, reg: u8, max: usize) -> Result<Vec<u8>> {
        let req = proto::request(proto::OP_GET, reg, &[], false);
        self.stream.write_all(&req).context("write blob request")?;

        let mut out = Vec::new();
        let mut chunk = [0u8; 2048];
        while out.len() < max {
            match self.stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    break
                }
                Err(e) => return Err(e).context("read blob"),
            }
        }
        Ok(out)
    }

    /// Format a reply for humans, resolving the register name.
    pub fn describe(&self, reg: u8, reply: &Reply) -> String {
        let name = crate::regs::by_addr(reg).map(|r| r.name).unwrap_or("?");
        match reply {
            Reply::Value { payload, .. } => {
                format!("{name:<18} 0x{reg:02x} = {}", Hex(payload))
            }
            other => format!("{name:<18} 0x{reg:02x} : {other}"),
        }
    }
}

fn resolve(host: &str) -> Result<SocketAddr> {
    let with_port = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:{}", proto::PORT)
    };
    with_port
        .to_socket_addrs()
        .with_context(|| format!("resolve {with_port}"))?
        .next()
        .ok_or_else(|| anyhow!("no address for {with_port}"))
}

fn is_transient(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::ConnectionRefused | ErrorKind::AddrInUse | ErrorKind::ConnectionReset
    )
}
