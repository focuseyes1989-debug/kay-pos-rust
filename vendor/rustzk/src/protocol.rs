use crate::constants::*;
use crate::{ZKError, ZKResult};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::borrow::Cow;
use std::io::{self, Cursor};

/// Calculates the ZK protocol checksum for raw byte data (default algorithm).
///
/// This uses the Python pyzk-aligned algorithm: `!(sum as i32)` then adding
/// `USHRT_MAX` until non-negative. This produces a result that is exactly 1 less
/// than the legacy Rust bitwise NOT approach.
///
/// For packet-level checksums, prefer [`ZKPacket::new`] or [`ZKPacket::new_with_legacy`].
pub fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    if data.len() >= 8 {
        // Sum bytes 0-1 (command) and bytes 4+ (session_id, reply_id, payload),
        // skipping bytes 2-3 which hold the checksum field itself.
        sum = sum.wrapping_add(sum_u16_pairs(&data[..2]));
        sum = sum.wrapping_add(sum_u16_pairs(&data[4..]));
    } else {
        sum = sum.wrapping_add(sum_u16_pairs(data));
    }
    finalize_checksum(sum)
}

/// Sum a byte slice as little-endian u16 pairs, wrapping at `USHRT_MAX`.
/// Used internally by checksum calculation functions.
fn sum_u16_pairs(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0;
    let len = data.len();
    while i + 1 < len {
        let val = u16::from_le_bytes([data[i], data[i + 1]]);
        sum += val as u32;
        if sum > USHRT_MAX as u32 {
            sum -= USHRT_MAX as u32;
        }
        i += 2;
    }
    if i < len {
        sum += data[i] as u32;
        if sum > USHRT_MAX as u32 {
            sum -= USHRT_MAX as u32;
        }
    }
    while sum > USHRT_MAX as u32 {
        sum -= USHRT_MAX as u32;
    }
    sum
}

/// Default checksum finalization: signed negation (Python pyzk-compatible).
///
/// Python's `~x` on a signed int gives `-(x+1)`, which after adding 65535
/// yields `65534 - x`. This differs by exactly 1 from the legacy approach.
fn finalize_checksum(sum: u32) -> u16 {
    let mut checksum = !(sum as i32);
    while checksum < 0 {
        checksum += USHRT_MAX as i32;
    }
    checksum as u16
}

/// Legacy checksum finalization: Rust's native unsigned bitwise NOT on u16.
///
/// `!(sum as u16) = 65535 - sum`. Produces a checksum exactly **1 greater**
/// than [`finalize_checksum`] for all inputs. Required by older firmware
/// (e.g., ZAM180_TFT) that strictly validates checksums.
fn finalize_checksum_legacy(mut sum: u32) -> u16 {
    while sum > USHRT_MAX as u32 {
        sum -= USHRT_MAX as u32;
    }
    !(sum as u16)
}

/// Represents a ZK protocol packet.
#[derive(Debug, Clone)]
pub struct ZKPacket<'a> {
    /// The command code (e.g., CMD_CONNECT).
    command: u16,
    /// The packet checksum.
    checksum: u16,
    /// The session ID allocated by the device.
    session_id: u16,
    /// The reply ID for tracking request-response pairs.
    reply_id: u16,
    /// The raw payload of the command.
    payload: Cow<'a, [u8]>,
}

impl<'a> ZKPacket<'a> {
    /// Getter for command.
    pub fn command(&self) -> u16 {
        self.command
    }

    /// Getter for checksum.
    pub fn checksum(&self) -> u16 {
        self.checksum
    }

    /// Getter for session_id.
    pub fn session_id(&self) -> u16 {
        self.session_id
    }

    /// Getter for reply_id.
    pub fn reply_id(&self) -> u16 {
        self.reply_id
    }

    /// Getter for payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the packet and returns the payload Cow.
    pub fn into_payload(self) -> Cow<'a, [u8]> {
        self.payload
    }

    /// Verifies if the packet's checksum matches the calculated one.
    pub fn verify_checksum(&self, use_legacy: bool) -> bool {
        let expected = if use_legacy {
            self.calculate_checksum_legacy()
        } else {
            self.calculate_checksum()
        };
        self.checksum == expected
    }
    /// Creates a new ZKPacket and automatically calculates the checksum
    /// using the **default** (Python pyzk-aligned) algorithm.
    ///
    /// # Checksum Algorithm
    ///
    /// The default algorithm uses signed negation: `!(sum as i32)` then adds
    /// `USHRT_MAX` (65535) until non-negative. For a sum of 999, this produces:
    /// `!(999 as i32) = -1000`, then `-1000 + 65535 = 64535 (0xFC17)`.
    ///
    /// Compatible with firmware that follows the Python pyzk reference implementation.
    pub fn new(
        command: u16,
        session_id: u16,
        reply_id: u16,
        payload: impl Into<Cow<'a, [u8]>>,
    ) -> Self {
        let mut packet = ZKPacket {
            command,
            checksum: 0,
            session_id,
            reply_id,
            payload: payload.into(),
        };
        packet.checksum = packet.calculate_checksum();
        packet
    }

    /// Creates a new ZKPacket using the **legacy** checksum algorithm.
    ///
    /// # Checksum Algorithm
    ///
    /// The legacy algorithm uses Rust's unsigned bitwise NOT: `!(sum as u16)`.
    /// For a sum of 999, this produces: `!999u16 = 65535 - 999 = 64536 (0xFC18)`.
    ///
    /// This is exactly **1 greater** than the default algorithm for all inputs.
    ///
    /// # Firmware Compatibility
    ///
    /// Required by older ZKTeco firmware that validates checksums strictly:
    /// - **ZAM180_TFT** (Ver 6.60 Aug 19 2021) — confirmed via real device test
    /// - Likely other firmware released before ~2020
    ///
    /// These devices silently drop packets with the default checksum (no error
    /// response), causing connection timeouts.
    pub fn new_with_legacy(
        command: u16,
        session_id: u16,
        reply_id: u16,
        payload: impl Into<Cow<'a, [u8]>>,
    ) -> Self {
        let mut packet = ZKPacket {
            command,
            checksum: 0,
            session_id,
            reply_id,
            payload: payload.into(),
        };
        packet.checksum = packet.calculate_checksum_legacy();
        packet
    }

    fn calculate_checksum(&self) -> u16 {
        // Sum header fields (excluding checksum field itself) + payload bytes
        let mut sum = self.command as u32 + self.session_id as u32 + self.reply_id as u32;
        sum = sum.wrapping_add(sum_u16_pairs(&self.payload));
        finalize_checksum(sum)
    }

    /// Legacy checksum algorithm (v0.4.4 and earlier).
    ///
    /// Uses Rust's native unsigned bitwise NOT on u16: `!(sum as u16) = 65535 - sum`.
    /// This produces a checksum exactly **1 greater** than the default algorithm.
    ///
    /// # Why two algorithms?
    ///
    /// Python's `~x` on a signed int gives `-(x+1)`, which after adding 65535
    /// yields `65534 - x`. Rust's `!x` on a u16 yields `65535 - x`. The 1-bit
    /// difference causes some firmware (e.g., ZAM180) to reject packets that
    /// use the default algorithm.
    fn calculate_checksum_legacy(&self) -> u16 {
        let mut sum = self.command as u32 + self.session_id as u32 + self.reply_id as u32;
        sum = sum.wrapping_add(sum_u16_pairs(&self.payload));
        finalize_checksum_legacy(sum)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + self.payload.len());
        let _ = self.to_bytes_into(&mut buf);
        buf
    }

    pub fn to_bytes_into(&self, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.write_u16::<LittleEndian>(self.command)?;
        buf.write_u16::<LittleEndian>(self.checksum)?;
        buf.write_u16::<LittleEndian>(self.session_id)?;
        buf.write_u16::<LittleEndian>(self.reply_id)?;
        buf.extend_from_slice(&self.payload);
        Ok(())
    }

    pub fn from_bytes(data: &'a [u8]) -> io::Result<Self> {
        if data.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Packet too short",
            ));
        }

        // Validate packet size bounds to catch malformed data early
        if let Err(e) = crate::security::validate_packet_size(data.len()) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
        }
        let mut rdr = Cursor::new(data);
        let command = rdr.read_u16::<LittleEndian>()?;
        let checksum = rdr.read_u16::<LittleEndian>()?;
        let session_id = rdr.read_u16::<LittleEndian>()?;
        let reply_id = rdr.read_u16::<LittleEndian>()?;
        let payload = Cow::Borrowed(&data[8..]);
        Ok(ZKPacket {
            command,
            checksum,
            session_id,
            reply_id,
            payload,
        })
    }

    pub fn from_bytes_owned(mut data: Vec<u8>) -> ZKResult<Self> {
        if data.len() < 8 {
            return Err(ZKError::InvalidData(
                crate::ZKErrorCode::InvalidDataFormat,
                "Packet too short".into(),
            ));
        }

        let (command, checksum, session_id, reply_id) = {
            let mut rdr = Cursor::new(&data);
            let cmd = rdr.read_u16::<LittleEndian>().map_err(ZKError::from)?;
            let chk = rdr.read_u16::<LittleEndian>().map_err(ZKError::from)?;
            let sid = rdr.read_u16::<LittleEndian>().map_err(ZKError::from)?;
            let rid = rdr.read_u16::<LittleEndian>().map_err(ZKError::from)?;
            (cmd, chk, sid, rid)
        };

        // split_off is more efficient as it doesn't shift the entire vector
        let payload = data.split_off(8);

        Ok(ZKPacket {
            command,
            checksum,
            session_id,
            reply_id,
            payload: Cow::Owned(payload),
        })
    }
}

pub struct TCPWrapper;

impl TCPWrapper {
    pub fn wrap(packet: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + packet.len());
        let _ = Self::wrap_into(packet, &mut buf);
        buf
    }

    pub fn wrap_into(packet: &[u8], buf: &mut Vec<u8>) -> io::Result<()> {
        buf.write_u16::<LittleEndian>(MACHINE_PREPARE_DATA_1)?;
        buf.write_u16::<LittleEndian>(MACHINE_PREPARE_DATA_2)?;
        buf.write_u32::<LittleEndian>(packet.len() as u32)?;
        buf.extend_from_slice(packet);
        Ok(())
    }

    pub fn unwrap(data: &[u8]) -> io::Result<(&[u8], usize)> {
        let (_length, total_len) = Self::decode_header(data)?;

        if data.len() < total_len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "TCP packet incomplete",
            ));
        }

        Ok((&data[8..total_len], total_len))
    }

    pub fn decode_header(data: &[u8]) -> io::Result<(usize, usize)> {
        if data.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "TCP header too short",
            ));
        }
        let mut rdr = Cursor::new(data);
        let magic1 = rdr.read_u16::<LittleEndian>()?;
        let magic2 = rdr.read_u16::<LittleEndian>()?;
        let length = rdr.read_u32::<LittleEndian>()? as usize;

        if magic1 != MACHINE_PREPARE_DATA_1 || magic2 != MACHINE_PREPARE_DATA_2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid TCP magic numbers",
            ));
        }

        Ok((length, 8 + length))
    }
}

#[cfg(test)]
mod tests {
    pub use super::*;

    #[test]
    fn test_calculate_checksum() {
        // Sample packet: Connect command (1000), session 0, reply USHRT_MAX-1
        // Header: [E8, 03, 00, 00, 00, 00, FE, FF]
        // CMD: 1000 (0x03E8)
        // Checksum placeholder: 0
        // Session: 0
        // Reply: 65534 (0xFFFE)
        let data = vec![0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, 0xFE, 0xFF];
        let checksum = calculate_checksum(&data);
        // Sum: 0x03E8 + 0x0000 + 0x0000 + 0xFFFE = 0x103E6
        // Adjusted: 0x103E6 - 0xFFFF = 0x03E7 (999)
        // Python style NOT: 65534 - 999 = 64535 (0xFC17)
        assert_eq!(checksum, 0xFC17);
    }

    #[test]
    fn test_zk_packet_new() {
        let packet = ZKPacket::new(1000, 0, 65534, vec![]);
        assert_eq!(packet.command, 1000);
        assert_eq!(packet.checksum, 0xFC17);
        assert_eq!(packet.session_id, 0);
        assert_eq!(packet.reply_id, 65534);
    }

    #[test]
    fn test_zk_packet_serialization() {
        let packet = ZKPacket::new(1000, 0, 65534, vec![0x01, 0x02]);
        let bytes = packet.to_bytes();
        let decoded = ZKPacket::from_bytes(&bytes).unwrap();
        assert_eq!(packet.command, decoded.command);
        assert_eq!(packet.checksum, decoded.checksum);
        assert_eq!(packet.payload, decoded.payload);
    }

    #[test]
    fn test_tcp_wrapper() {
        let packet = vec![0xAA, 0xBB];
        let wrapped = TCPWrapper::wrap(&packet);
        assert_eq!(wrapped.len(), 10); // 8 bytes header + 2 bytes data
        let (unwrapped, total_len) = TCPWrapper::unwrap(&wrapped).unwrap();
        assert_eq!(unwrapped, packet);
        assert_eq!(total_len, 10);
    }

    // ── Unit tests for checksum helpers ─────────────────────────────────

    #[test]
    fn test_sum_u16_pairs_empty() {
        assert_eq!(sum_u16_pairs(&[]), 0);
    }

    #[test]
    fn test_sum_u16_pairs_single_byte() {
        // Single byte 0x42 → 66
        assert_eq!(sum_u16_pairs(&[0x42]), 66);
    }

    #[test]
    fn test_sum_u16_pairs_even_aligned() {
        // Two bytes: [0x01, 0x02] → u16 LE = 0x0201 = 513
        assert_eq!(sum_u16_pairs(&[0x01, 0x02]), 513);
    }

    #[test]
    fn test_sum_u16_pairs_odd_length() {
        // [0x01, 0x00, 0xFF] → u16 pair (0x0001=1) + single byte (0xFF=255) = 256
        assert_eq!(sum_u16_pairs(&[0x01, 0x00, 0xFF]), 256);
    }

    #[test]
    fn test_sum_u16_pairs_overflow_wraps_at_ushrt_max() {
        // Two u16 pairs: 65535 + 1 = 65536, should wrap to 1
        let data = [0xFF, 0xFF, 0x01, 0x00]; // 65535 + 1
        assert_eq!(sum_u16_pairs(&data), 1);
    }

    #[test]
    fn test_sum_u16_pairs_multi_overflow() {
        // 65535 + 65535 + 65535 = 196605
        // After first add: 65535, no wrap (65535 ≤ 65535)
        // After second: 65535 + 65535 = 131070 → 131070 - 65535 = 65535
        // After third: 65535 + 65535 = 131070 → 131070 - 65535 = 65535
        // Then while > USHRT_MAX: 65535 ≤ 65535, done
        // Result: 65535
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(sum_u16_pairs(&data), 65535);
    }

    #[test]
    fn test_finalize_checksum_zero_sum() {
        // sum=0 → !(0 as i32) = -1 → -1 + 65535 = 65534
        assert_eq!(finalize_checksum(0), 0xFFFE);
    }

    #[test]
    fn test_finalize_checksum_ushrt_max() {
        // sum=65535 → !(65535 as i32) = -65536 → -65536 + 65535 = -1 → +65535 = 65534
        assert_eq!(finalize_checksum(65535), 0xFFFE);
    }

    #[test]
    fn test_finalize_checksum_legacy_wraps_before_not() {
        // sum=66534 > USHRT_MAX, legacy must wrap first:
        // 66534 - 65535 = 999, then !(999 as u16) = 65535 - 999 = 64536 (0xFC18)
        assert_eq!(finalize_checksum_legacy(66534), 0xFC18);
    }

    #[test]
    fn test_finalize_legacy_at_boundary() {
        // sum=65535 → no wrap needed, !(65535 as u16) = 0
        assert_eq!(finalize_checksum_legacy(65535), 0);
    }

    #[test]
    fn test_both_finalizers_differ_by_one() {
        // The "differ by one" property holds for all sums except exact multiples
        // of USHRT_MAX (where the wrap-to-zero edge case causes a difference of 2).
        for sum in [0u32, 1, 100, 999, 65534, 65536] {
            let default = finalize_checksum(sum);
            let legacy = finalize_checksum_legacy(sum);
            assert_eq!(
                legacy.wrapping_sub(default),
                1,
                "sum={}: default={:#06X}, legacy={:#06X}",
                sum,
                default,
                legacy
            );
        }
    }

    #[test]
    fn test_finalizers_at_multiple_of_ushrt_max() {
        // At exact multiples of USHRT_MAX, both algorithms agree after wrapping
        // but the difference is 2 due to the wrap-to-zero edge case.
        for sum in [65535u32, 131070] {
            let default = finalize_checksum(sum);
            let legacy = finalize_checksum_legacy(sum);
            assert_eq!(
                legacy.wrapping_sub(default),
                2,
                "sum={}: default={:#06X}, legacy={:#06X}",
                sum,
                default,
                legacy
            );
        }
    }
}
