//! Internal connection state and configuration for the ZK client.
//!
//! These types are `pub(crate)` — they are implementation details of the
//! [`ZK`](crate::ZK) struct and not part of the public API.

use std::time::Duration;

use crate::constants::USHRT_MAX;
use crate::transport::ZKTransport;

/// Internal transport-level state: socket, session, and I/O buffers.
pub(crate) struct ZKConnection {
    pub(crate) transport: Option<ZKTransport>,
    pub(crate) is_connected: bool,
    pub(crate) addr: String,
    pub(crate) session_id: u16,
    pub(crate) reply_id: u16,
    pub(crate) use_legacy_checksum: bool,
    /// Reusable buffer for UDP reads to avoid per-packet heap allocation.
    pub(crate) udp_buf: Vec<u8>,
    /// Reusable buffer for packet serialization to avoid heap allocations on write.
    pub(crate) write_buf: Vec<u8>,
}

impl ZKConnection {
    pub(crate) fn new(addr: String) -> Self {
        ZKConnection {
            transport: None,
            is_connected: false,
            addr,
            session_id: 0,
            reply_id: USHRT_MAX - 1,
            use_legacy_checksum: false,
            udp_buf: vec![0u8; 2048],
            write_buf: Vec::with_capacity(1024),
        }
    }
}

/// Internal client configuration: password, timeout, encoding, and user-packet size.
pub(crate) struct ZKConfig {
    pub(crate) password: u32,
    pub(crate) timeout: Duration,
    pub(crate) encoding: &'static str,
    pub(crate) user_packet_size: usize,
}

impl ZKConfig {
    pub(crate) fn new() -> Self {
        ZKConfig {
            password: 0,
            timeout: Duration::from_secs(60),
            encoding: "UTF-8",
            user_packet_size: 28,
        }
    }
}
