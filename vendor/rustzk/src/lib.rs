#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod constants;
pub use crate::constants::*;
pub mod attendance;
pub mod connection;
pub mod device;
pub mod models;
pub mod protocol;
pub mod security;
pub mod transport;
pub mod user;

use crate::connection::{ZKConfig, ZKConnection};
use byteorder::ReadBytesExt;
use chrono::{Datelike, Timelike};
use std::io;
use std::time::Duration;
use thiserror::Error;

pub use crate::transport::ZKTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZKErrorCode {
    Unauthorized,
    Timeout,
    ChecksumMismatch,
    InvalidSession,
    BufferOverflow,
    ProtocolViolation,
    ConnectionFailed,
    DataConflict,
    InvalidDataFormat,
    Other,
}

#[derive(Error, Debug)]
pub enum ZKError {
    #[error("Network error: {0}")]
    Network(#[from] io::Error),
    #[error("Connection error ({0:?}): {1}")]
    Connection(ZKErrorCode, String),
    #[error("Response error ({0:?}): {1}")]
    Response(ZKErrorCode, String),
    #[error("Invalid data ({0:?}): {1}")]
    InvalidData(ZKErrorCode, String),
}

pub type ZKResult<T> = Result<T, ZKError>;

impl ZKError {
    /// Checks if the error is due to a network connection timeout.
    pub fn is_timeout(&self) -> bool {
        match self {
            ZKError::Network(err) => {
                err.kind() == std::io::ErrorKind::TimedOut
                    || err.kind() == std::io::ErrorKind::WouldBlock
            }
            ZKError::Connection(code, _) | ZKError::Response(code, _) => {
                *code == ZKErrorCode::Timeout
            }
            _ => false,
        }
    }

    /// Checks if the error is due to authorization failure.
    pub fn is_unauthorized(&self) -> bool {
        match self {
            ZKError::Connection(code, _) => *code == ZKErrorCode::Unauthorized,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZKProtocol {
    TCP,
    UDP,
    Auto,
}

pub struct ZK {
    config: ZKConfig,
    connection: ZKConnection,
    user_id_cache: Option<std::collections::HashMap<u16, String>>,
    device_info: Option<models::DeviceInfo>,
    timezone_offset: i32, // Offset in minutes
    timezone_synced: bool,
}

impl ZK {
    pub fn new(addr: &str, port: u16) -> Self {
        ZK {
            config: ZKConfig::new(),
            connection: ZKConnection::new(format!("{}:{}", addr, port)),
            user_id_cache: None,
            device_info: None,
            timezone_offset: 0,
            timezone_synced: false,
        }
    }

    /// Sets the communication password for the device.
    pub fn set_password(&mut self, password: u32) {
        self.config.password = password;
    }

    /// Forces use of the legacy checksum algorithm (Rust bitwise NOT).
    ///
    /// By default, rustzk tries the default checksum first and automatically
    /// falls back to legacy on timeout. Call this before [`connect`](Self::connect)
    /// to skip the auto-detection and connect immediately with legacy checksum.
    pub fn set_legacy_checksum(&mut self, legacy: bool) {
        self.connection.use_legacy_checksum = legacy;
    }

    /// Returns the device address.
    pub fn addr(&self) -> &str {
        &self.connection.addr
    }

    /// Sets the device address.
    pub fn set_addr(&mut self, addr: String) {
        self.connection.addr = addr;
    }

    /// Returns the read/write timeout.
    pub fn timeout(&self) -> Duration {
        self.config.timeout
    }

    /// Sets the read/write timeout.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.config.timeout = timeout;
    }

    /// Returns the user packet size.
    pub fn user_packet_size(&self) -> usize {
        self.config.user_packet_size
    }

    /// Sets the user packet size.
    pub fn set_user_packet_size(&mut self, size: usize) {
        self.config.user_packet_size = size;
    }

    /// Returns the string encoding used for decoding display names.
    pub fn encoding(&self) -> &'static str {
        self.config.encoding
    }

    /// Internal helper to generate the authentication communication key.
    fn make_commkey(key: u32, session_id: u16, ticks: u8) -> Vec<u8> {
        let mut k = 0u32;
        for i in 0..32 {
            if (key & (1 << i)) != 0 {
                k = (k << 1) | 1;
            } else {
                k <<= 1;
            }
        }
        k = k.wrapping_add(session_id as u32);

        let b1 = (k & 0xFF) as u8 ^ b'Z';
        let b2 = ((k >> 8) & 0xFF) as u8 ^ b'K';
        let b3 = ((k >> 16) & 0xFF) as u8 ^ b'S';
        let b4 = ((k >> 24) & 0xFF) as u8 ^ b'O';

        let k = (b1 as u16) | ((b2 as u16) << 8);
        let k2 = (b3 as u16) | ((b4 as u16) << 8);

        let c1 = (k2 & 0xFF) as u8 ^ ticks; // b3 ^ ticks
        let c2 = ((k2 >> 8) & 0xFF) as u8 ^ ticks; // b4 ^ ticks
        let c3 = ticks;
        let c4 = ((k >> 8) & 0xFF) as u8 ^ ticks; // b2 ^ ticks

        vec![c1, c2, c3, c4]
    }

    /// Checks if the connection is still alive by sending a lightweight `CMD_GET_TIME` request to the device.
    /// Returns `true` if the device responds successfully, and `false` otherwise.
    /// If the connection has died, `is_connected` is automatically set to `false`.
    pub fn is_alive(&mut self) -> bool {
        if !self.connection.is_connected {
            return false;
        }
        let saved_timeout = self.config.timeout;
        self.set_transport_read_timeout(Duration::from_secs(2));
        let res = self.send_command(CMD_GET_TIME, &[]);
        self.set_transport_read_timeout(saved_timeout);

        match res {
            Ok(packet) => packet.command() == CMD_ACK_OK || packet.command() == CMD_ACK_DATA,
            Err(_) => {
                self.connection.is_connected = false;
                self.connection.transport = None;
                false
            }
        }
    }

    pub fn session_id(&self) -> u16 {
        self.connection.session_id
    }
    pub fn reply_id(&self) -> u16 {
        self.connection.reply_id
    }
    pub fn use_legacy_checksum(&self) -> bool {
        self.connection.use_legacy_checksum
    }

    /// Decodes a GBK-encoded byte slice into a String.
    pub(crate) fn decode_gbk(bytes: &[u8]) -> String {
        let trimmed = bytes
            .iter()
            .position(|&x| x == 0)
            .map_or(bytes, |i| &bytes[..i]);
        let (cow, _encoding, has_malformed) = encoding_rs::GBK.decode(trimmed);
        if has_malformed {
            log::warn!(
                "GBK decoding encountered malformed sequences in data: {:?}",
                trimmed
            );
        }
        cow.into_owned()
    }

    pub fn decode_time(t: &[u8]) -> ZKResult<chrono::NaiveDateTime> {
        if t.len() < 4 {
            return Err(ZKError::InvalidData(
                ZKErrorCode::InvalidDataFormat,
                "Timestamp too short".into(),
            ));
        }
        let mut rdr = io::Cursor::new(t);
        let t = rdr.read_u32::<byteorder::LittleEndian>()?;

        let second = t % 60;
        let t = t / 60;
        let minute = t % 60;
        let t = t / 60;
        let hour = t % 24;
        let t = t / 24;
        let day = t % 31 + 1;
        let t = t / 31;
        let month = t % 12 + 1;
        let t = t / 12;
        let year = (t + 2000) as i32;

        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d: chrono::NaiveDate| d.and_hms_opt(hour, minute, second))
            .ok_or_else(|| {
                ZKError::InvalidData(ZKErrorCode::InvalidDataFormat, "Invalid date/time".into())
            })
    }

    pub fn encode_time(t: chrono::NaiveDateTime) -> u32 {
        let year = (t.year() % 100) as u32;
        let month = t.month();
        let day = t.day();
        let hour = t.hour();
        let minute = t.minute();
        let second = t.second();

        (year * 12 * 31 + (month - 1) * 31 + day - 1) * (24 * 60 * 60)
            + (hour * 60 + minute) * 60
            + second
    }

    /// Helper to find the next available UID on the device.
    /// `start_uid`: The UID to start searching from (useful for testing in high ranges).
    pub fn get_next_free_uid(&mut self, start_uid: u16) -> ZKResult<u16> {
        let users = self.get_users()?;
        let used_uids: std::collections::HashSet<u16> = users.iter().map(|u| u.uid()).collect();

        for uid in start_uid..=65535 {
            if !used_uids.contains(&uid) {
                return Ok(uid);
            }
        }

        Err(ZKError::Response(
            ZKErrorCode::Other,
            "No free UID found in the specified range".into(),
        ))
    }
}

impl Drop for ZK {
    fn drop(&mut self) {
        if self.connection.is_connected {
            let _ = self.send_exit_packet();
            self.connection.is_connected = false;
        }
        self.connection.transport = None;
    }
}

/// Convenience helper for examples and CLI tools: parse IP address and
/// optional port from command-line arguments.
///
/// Returns `(ip, port)` using default port 4370 when none is given.
/// Prints usage to stderr and exits with code 1 when arguments are missing.
///
/// # Example
///
/// ```no_run
/// let (ip, port) = rustzk::parse_ip_port();
/// let mut zk = rustzk::ZK::new(&ip, port);
/// ```
pub fn parse_ip_port() -> (String, u16) {
    let args: Vec<String> = std::env::args().collect();
    match parse_ip_port_from_args(&args) {
        Ok(result) => result,
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    }
}

/// Pure parsing logic for `parse_ip_port`, separated so it can be unit-tested.
/// Takes program arguments as a slice and returns either the parsed `(ip, port)`
/// or an error message suitable for stderr.
pub(crate) fn parse_ip_port_from_args(args: &[String]) -> Result<(String, u16), String> {
    if args.len() < 2 {
        return Err(format!("Usage: {} <ip> [port]", args[0]));
    }
    let ip = args[1].clone();
    let port = if args.len() > 2 {
        args[2].parse().unwrap_or(4370)
    } else {
        4370
    };
    Ok((ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_commkey() {
        // Key: 0, Session: 619, Ticks: 50
        // Expected: [97, 125, 50, 123]
        let key = 0;
        let session_id = 619;
        let ticks = 50;
        let result = ZK::make_commkey(key, session_id, ticks);
        assert_eq!(result, vec![97, 125, 50, 123]);
    }

    #[test]
    fn test_zk_new_default_password() {
        let zk = ZK::new("192.168.1.201", 4370);
        assert_eq!(zk.config.password, 0);
    }

    #[test]
    fn test_zk_set_password() {
        let mut zk = ZK::new("192.168.1.201", 4370);
        zk.set_password(12345);
        assert_eq!(zk.config.password, 12345);
    }

    #[test]
    fn test_make_commkey_complex() {
        // Key: 12345, Session: 9999, Ticks: 100
        let key = 12345;
        let session_id = 9999;
        let ticks = 100;
        let result = ZK::make_commkey(key, session_id, ticks);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_zk_uncovered_getters_setters() {
        let mut zk = ZK::new("192.168.1.201", 4370);
        assert_eq!(zk.addr(), "192.168.1.201:4370");
        zk.set_addr("10.0.0.1:4370".to_string());
        assert_eq!(zk.addr(), "10.0.0.1:4370");

        assert_eq!(zk.timeout(), Duration::from_secs(60));
        zk.set_timeout(Duration::from_secs(15));
        assert_eq!(zk.timeout(), Duration::from_secs(15));

        assert_eq!(zk.user_packet_size(), 28);
        zk.set_user_packet_size(72);
        assert_eq!(zk.user_packet_size(), 72);

        assert_eq!(zk.encoding(), "UTF-8");
        assert!(!zk.is_alive());
    }

    #[test]
    fn test_zk_device_info_getters_return_zero_when_none() {
        let zk = ZK::new("192.168.1.201", 4370);
        // Before read_sizes(), device_info is None — all getters should return 0.
        assert_eq!(zk.users(), 0);
        assert_eq!(zk.users_cap(), 0);
        assert_eq!(zk.fingers(), 0);
        assert_eq!(zk.fingers_cap(), 0);
        assert_eq!(zk.records(), 0);
        assert_eq!(zk.records_cap(), 0);
        assert_eq!(zk.faces(), 0);
        assert_eq!(zk.faces_cap(), 0);
        assert_eq!(zk.cards(), 0);
    }

    #[test]
    fn test_get_user_id_from_cache_fallback_on_error() {
        let mut zk = ZK::new("127.0.0.1", 4370);
        assert!(zk.user_id_cache.is_none());

        // Calling get_user_id_from_cache when not connected triggers failure on refresh_user_cache.
        // It must initialize an empty cache and fallback to returning the UID as a string.
        let res = zk.get_user_id_from_cache(12);
        assert_eq!(res, "12");
        assert!(zk.user_id_cache.is_some());
        assert!(zk.user_id_cache.as_ref().unwrap().is_empty());
    }

    // ── parse_ip_port_from_args tests ───────────────────────────────────

    fn make_args(bin: &str, extra: &[&str]) -> Vec<String> {
        let mut v = vec![bin.to_string()];
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn test_parse_ip_only_default_port() {
        let args = make_args("myapp", &["192.168.1.1"]);
        let (ip, port) = parse_ip_port_from_args(&args).unwrap();
        assert_eq!(ip, "192.168.1.1");
        assert_eq!(port, 4370);
    }

    #[test]
    fn test_parse_ip_with_port() {
        let args = make_args("myapp", &["10.0.0.1", "3000"]);
        let (ip, port) = parse_ip_port_from_args(&args).unwrap();
        assert_eq!(ip, "10.0.0.1");
        assert_eq!(port, 3000);
    }

    #[test]
    fn test_parse_invalid_port_falls_back() {
        let args = make_args("myapp", &["10.0.0.1", "not-a-port"]);
        let (ip, port) = parse_ip_port_from_args(&args).unwrap();
        assert_eq!(ip, "10.0.0.1");
        assert_eq!(port, 4370);
    }

    #[test]
    fn test_parse_missing_ip_returns_err() {
        let args = make_args("myapp", &[]);
        let err = parse_ip_port_from_args(&args).unwrap_err();
        assert!(err.contains("Usage:"));
        assert!(err.contains("myapp"));
    }
}
