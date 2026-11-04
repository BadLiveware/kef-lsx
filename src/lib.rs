//! Unofficial control library for first-generation KEF LSX and LS50 Wireless
//! speakers.
//!
//! The speakers expose an undocumented binary control protocol on TCP 50001
//! with no authentication, no session, and (until now) no published spec.
//! This crate implements it, with the checksum and framing documented in
//! [`proto`].
//!
//! ```no_run
//! use kef_lsx::{Client, state::Source};
//! use std::time::Duration;
//!
//! # fn main() -> anyhow::Result<()> {
//! let mut kef = Client::connect("192.0.2.10", Duration::from_secs(3))?;
//! let reply = kef.get(0x25)?;              // volume
//! let vol = kef_lsx::state::as_byte(&reply);
//! let source = kef.get(0x30)?;
//! if let Some(byte) = source.byte() {
//!     let src = Source::decode(byte);
//!     println!("volume {vol:?}, input {}", src.input);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Hold one [`Client`] for the process lifetime — see [`client`] for why.

pub mod client;
pub mod proto;
pub mod regs;
pub mod state;

pub use client::Client;
pub use proto::{Reply, PORT};
pub use regs::{Kind, Reg};
