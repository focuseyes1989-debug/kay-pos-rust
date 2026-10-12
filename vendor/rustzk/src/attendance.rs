use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use std::collections::HashMap;
use std::io::{self, Read};

use crate::constants::*;
use crate::models::Attendance;
use crate::protocol::ZKPacket;
use crate::{ZKError, ZKErrorCode, ZKResult, ZK};

/// Parsed real-time event data extracted from a REG_EVENT packet payload.
#[derive(Debug)]
struct EventData {
    uid: u32,
    user_id: String,
    status: u8,
    punch: u8,
    timestamp: chrono::NaiveDateTime,
}

/// Parse a REG_EVENT payload into structured [`EventData`].
///
/// Supports three wire formats:
/// - 10-byte: uid(u16) + status(u8) + punch(u8) + time(6)
/// - 12-byte: user_id_num(u32) + status(u8) + punch(u8) + time(6)
/// - 32-byte: user_id_str(24) + status(u8) + punch(u8) + time(6) + ...
///
/// For 10/12-byte formats the returned `user_id` is the raw uid as a string;
/// the caller should resolve it via the user-id cache. For 32-byte format
/// the `user_id` is already the final string.
fn parse_event_data(data: &[u8]) -> ZKResult<EventData> {
    if data.len() == EVENT_DATA_LEN_10 {
        let uid = LittleEndian::read_u16(&data[0..2]) as u32;
        let status = data[2];
        let punch = data[3];
        let timehex = &data[4..10];
        let timestamp = ZK::decode_timehex(timehex)?;
        Ok(EventData {
            uid,
            user_id: uid.to_string(),
            status,
            punch,
            timestamp,
        })
    } else if data.len() == EVENT_DATA_LEN_12 {
        let user_id_num = LittleEndian::read_u32(&data[0..4]);
        let status = data[4];
        let punch = data[5];
        let timehex = &data[6..12];
        let timestamp = ZK::decode_timehex(timehex)?;
        Ok(EventData {
            uid: user_id_num,
            user_id: user_id_num.to_string(),
            status,
            punch,
            timestamp,
        })
    } else if data.len() >= EVENT_DATA_LEN_32 {
        let user_id = String::from_utf8_lossy(&data[0..24])
            .trim_matches('\0')
            .to_string();
        let status = data[24];
        let punch = data[25];
        let timehex = &data[26..32];
        let timestamp = ZK::decode_timehex(timehex)?;
        Ok(EventData {
            uid: 0,
            user_id,
            status,
            punch,
            timestamp,
        })
    } else {
        Err(ZKError::InvalidData(
            ZKErrorCode::InvalidDataFormat,
            format!("Unknown event data length: {}", data.len()),
        ))
    }
}

impl ZK {
    /// Retrieves all attendance records from the device.
    pub fn get_attendance(&mut self) -> ZKResult<Vec<Attendance>> {
        self.read_sizes()?;
        if self.records() == 0 {
            return Ok(Vec::new());
        }

        // Fetch raw attendance buffer FIRST, before any other buffer commands.
        let attendance_data = self.read_with_buffer(CMD_ATTLOG_RRQ, 0, 0)?;
        if attendance_data.len() < 4 {
            return Ok(Vec::new());
        }

        let total_size = LittleEndian::read_u32(&attendance_data[0..4]) as usize;
        if total_size > MAX_RESPONSE_SIZE {
            return Err(ZKError::InvalidData(
                ZKErrorCode::BufferOverflow,
                format!(
                    "Attendance data total_size {} exceeds maximum {}",
                    total_size, MAX_RESPONSE_SIZE
                ),
            ));
        }

        let record_size = detect_record_size(total_size, self.records() as usize);
        let data = &attendance_data[4..];

        if !can_parse_record_size(record_size, total_size) {
            return Err(ZKError::InvalidData(
                ZKErrorCode::InvalidDataFormat,
                format!(
                    "Unsupported or invalid attendance record size: {}",
                    record_size
                ),
            ));
        }

        let capacity = std::cmp::min(self.records() as usize, data.len() / record_size);
        let mut attendances = Vec::with_capacity(capacity);

        let mut uid_cache: HashMap<u32, String> = HashMap::new();
        let mut bytes_cache: HashMap<[u8; 24], String> = HashMap::new();
        let tz = self.timezone_offset;

        // Ensure device user-ID cache is populated for 8-byte record resolution.
        // This mirrors the lazy-load behavior of get_user_id_from_cache().
        if self.user_id_cache.is_none() {
            if let Err(e) = self.refresh_user_cache() {
                log::warn!(
                    "Failed to refresh user cache before parsing attendance: {}",
                    e
                );
                self.user_id_cache = Some(HashMap::new());
            }
        }
        let device_cache = self.user_id_cache.clone().unwrap_or_default();

        let is_8 = record_size_is(record_size, total_size, ATT_RECORD_SIZE_8);
        let is_16 = !is_8 && record_size_is(record_size, total_size, ATT_RECORD_SIZE_16);
        let is_40 = !is_8 && !is_16 && record_size >= ATT_RECORD_SIZE_40;

        if is_8 {
            parse_records_8(
                data,
                record_size,
                &mut attendances,
                &mut uid_cache,
                &device_cache,
                tz,
            )?;
        } else if is_16 {
            parse_records_16(data, record_size, &mut attendances, &mut uid_cache, tz)?;
        } else if is_40 {
            parse_records_40(data, record_size, &mut attendances, &mut bytes_cache, tz)?;
        }

        Ok(attendances)
    }

    /// Registers for specific real-time events.
    pub fn reg_event(&mut self, flags: u32) -> ZKResult<()> {
        let mut payload = [0u8; 4];
        byteorder::LittleEndian::write_u32(&mut payload, flags);

        let res = self.send_command(CMD_REG_EVENT, &payload)?;
        if res.command() == CMD_ACK_OK {
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                format!("Failed to register events with flags {}", flags),
            ))
        }
    }

    /// Internal helper to send a simple ACK_OK response.
    pub(crate) fn send_ack_ok(&mut self) -> ZKResult<()> {
        let packet = ZKPacket::new(
            CMD_ACK_OK,
            self.connection.session_id,
            self.connection.reply_id,
            &[],
        );
        self.send_packet(&packet)
    }

    /// Decodes a 6-byte compressed time format used in real-time events.
    fn decode_timehex(hex: &[u8]) -> ZKResult<chrono::NaiveDateTime> {
        if hex.len() < 6 {
            return Err(ZKError::InvalidData(
                ZKErrorCode::InvalidDataFormat,
                "Timehex too short".into(),
            ));
        }
        let year = hex[0] as i32 + 2000;
        let month = hex[1] as u32;
        let day = hex[2] as u32;
        let hour = hex[3] as u32;
        let minute = hex[4] as u32;
        let second = hex[5] as u32;

        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_opt(hour, minute, second))
            .ok_or_else(|| {
                ZKError::InvalidData(
                    ZKErrorCode::InvalidDataFormat,
                    "Invalid date/time in hex".into(),
                )
            })
    }

    /// Listens for real-time events and yields attendance records as they occur.
    /// This is a blocking call that will yield None on timeout.
    pub fn listen_events(&mut self) -> ZKResult<impl Iterator<Item = ZKResult<Attendance>> + '_> {
        self.reg_event(EF_ATTLOG)?;

        Ok(std::iter::from_fn(move || loop {
            match self.read_packet() {
                Ok(packet) => {
                    let _ = self.send_ack_ok();

                    if packet.command() != CMD_REG_EVENT {
                        return Some(Err(ZKError::Response(
                            ZKErrorCode::ProtocolViolation,
                            format!(
                                "Unexpected command during event listening: {}",
                                packet.command()
                            ),
                        )));
                    }

                    let data = packet.payload();
                    if data.is_empty() {
                        continue;
                    }

                    let event = match parse_event_data(data) {
                        Ok(e) => e,
                        Err(e) => return Some(Err(e)),
                    };

                    // Resolve user_id from cache for 10/12-byte formats.
                    // 32-byte format already carries the string user_id.
                    let user_id = if data.len() >= EVENT_DATA_LEN_32 {
                        event.user_id
                    } else {
                        self.get_user_id_from_cache(event.uid as u16)
                    };

                    return Some(Ok(Attendance::new(
                        event.uid,
                        user_id,
                        event.timestamp,
                        event.status,
                        event.punch,
                        self.timezone_offset,
                    )));
                }
                Err(ZKError::Network(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(e) => return Some(Err(e)),
            }
        }))
    }
}

// ── Private helpers for attendance record parsing ──────────────────────────

/// Detect the best record size from total_size and record count using
/// heuristic divisibility checks, preferring larger standard sizes.
fn detect_record_size(total_size: usize, records: usize) -> usize {
    let mut record_size = if records > 0 && total_size > 0 {
        total_size / records
    } else {
        0
    };

    if total_size > 0 {
        if total_size.is_multiple_of(40) && (record_size == 0 || record_size.is_multiple_of(40)) {
            record_size = 40;
        } else if total_size.is_multiple_of(16)
            && (record_size == 0 || record_size.is_multiple_of(16))
        {
            record_size = 16;
        } else if total_size.is_multiple_of(8)
            && (record_size == 0 || record_size.is_multiple_of(8))
        {
            record_size = 8;
        }
    }

    record_size
}

/// Returns true if the record_size is compatible with the standard size `std`,
/// considering modulo alignment.
fn record_size_is(record_size: usize, total_size: usize, std: usize) -> bool {
    if [ATT_RECORD_SIZE_8, ATT_RECORD_SIZE_16, ATT_RECORD_SIZE_40].contains(&record_size) {
        return record_size == std;
    }
    record_size == std
        || (record_size >= std && total_size.wrapping_rem(std) == 0 && record_size < std * 5)
}

/// Returns true if we have a parser for this record_size.
fn can_parse_record_size(record_size: usize, total_size: usize) -> bool {
    record_size_is(record_size, total_size, ATT_RECORD_SIZE_8)
        || record_size_is(record_size, total_size, ATT_RECORD_SIZE_16)
        || record_size >= ATT_RECORD_SIZE_40
}

/// Iterate over attendance records in `data`, calling `parse_chunk` for each
/// record-sized chunk. Handles the loop, offset tracking, and error propagation.
fn parse_records_with<F>(
    data: &[u8],
    record_size: usize,
    chunk_size: usize,
    out: &mut Vec<Attendance>,
    mut parse_chunk: F,
) -> ZKResult<()>
where
    F: FnMut(&[u8]) -> ZKResult<Attendance>,
{
    let mut offset = 0;
    while offset + chunk_size <= data.len() {
        let chunk = &data[offset..offset + chunk_size];
        out.push(parse_chunk(chunk)?);
        offset += record_size;
    }
    Ok(())
}

/// Parse attendance records in 8-byte format.
/// Layout: uid(u16) + status(u8) + time(u32) + punch(u8)
fn parse_records_8(
    data: &[u8],
    record_size: usize,
    out: &mut Vec<Attendance>,
    uid_cache: &mut HashMap<u32, String>,
    device_cache: &HashMap<u16, String>,
    tz_offset: i32,
) -> ZKResult<()> {
    parse_records_with(data, record_size, ATT_RECORD_SIZE_8, out, |chunk| {
        let mut rdr = io::Cursor::new(chunk);
        let uid = rdr.read_u16::<byteorder::LittleEndian>()?;
        let status = rdr.read_u8()?;
        let mut time_bytes = [0u8; 4];
        rdr.read_exact(&mut time_bytes)?;
        let punch = rdr.read_u8()?;

        let timestamp = ZK::decode_time(&time_bytes)?;
        let user_id = uid_cache.get(&(uid as u32)).cloned().unwrap_or_else(|| {
            let id = device_cache
                .get(&uid)
                .cloned()
                .unwrap_or_else(|| uid.to_string());
            uid_cache.insert(uid as u32, id.clone());
            id
        });

        Ok(Attendance::new(
            uid as u32, user_id, timestamp, status, punch, tz_offset,
        ))
    })
}

/// Parse attendance records in 16-byte format.
/// Layout: user_id(u32) + time(u32) + status(u8) + punch(u8) + reserved(2) + workcode(u32)
fn parse_records_16(
    data: &[u8],
    record_size: usize,
    out: &mut Vec<Attendance>,
    uid_cache: &mut HashMap<u32, String>,
    tz_offset: i32,
) -> ZKResult<()> {
    parse_records_with(data, record_size, ATT_RECORD_SIZE_16, out, |chunk| {
        let mut rdr = io::Cursor::new(chunk);
        let user_id_num = rdr.read_u32::<byteorder::LittleEndian>()?;
        let mut time_bytes = [0u8; 4];
        rdr.read_exact(&mut time_bytes)?;
        let status = rdr.read_u8()?;
        let punch = rdr.read_u8()?;

        let timestamp = ZK::decode_time(&time_bytes)?;
        let user_id = uid_cache.get(&user_id_num).cloned().unwrap_or_else(|| {
            let id = user_id_num.to_string();
            uid_cache.insert(user_id_num, id.clone());
            id
        });

        Ok(Attendance::new(
            user_id_num,
            user_id,
            timestamp,
            status,
            punch,
            tz_offset,
        ))
    })
}

/// Parse attendance records in 40-byte format.
/// Layout: uid(u16) + maybe_bom(10 bytes, optional) + user_id(24 bytes) +
///         status(u8) + time(u32) + punch(u8) + reserved
fn parse_records_40(
    data: &[u8],
    record_size: usize,
    out: &mut Vec<Attendance>,
    bytes_cache: &mut HashMap<[u8; 24], String>,
    tz_offset: i32,
) -> ZKResult<()> {
    parse_records_with(data, record_size, ATT_RECORD_SIZE_40, out, |chunk| {
        // Skip BOM-like prefix if present (e.g., b"\xff255\x00\x00\x00\x00\x00")
        let chunk = if chunk.starts_with(b"\xff255\x00\x00\x00\x00\x00") {
            &chunk[10..]
        } else {
            chunk
        };

        let mut rdr = io::Cursor::new(chunk);
        let uid = rdr.read_u16::<byteorder::LittleEndian>()?;
        let mut user_id_bytes = [0u8; 24];
        rdr.read_exact(&mut user_id_bytes)?;
        let status = rdr.read_u8()?;
        let mut time_bytes = [0u8; 4];
        rdr.read_exact(&mut time_bytes)?;
        let punch = rdr.read_u8()?;

        let timestamp = ZK::decode_time(&time_bytes)?;
        let user_id = bytes_cache.get(&user_id_bytes).cloned().unwrap_or_else(|| {
            let id = String::from_utf8_lossy(&user_id_bytes)
                .trim_matches('\0')
                .to_string();
            bytes_cache.insert(user_id_bytes, id.clone());
            id
        });

        Ok(Attendance::new(
            uid as u32, user_id, timestamp, status, punch, tz_offset,
        ))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn exact_record_formats_take_precedence_over_divisibility() {
        for size in [8, 16, 40] {
            for standard in [8, 16, 40] {
                assert_eq!(super::record_size_is(size, size * 3298, standard), size == standard);
            }
        }
    }
    use super::*;
    use byteorder::{LittleEndian, WriteBytesExt};

    #[test]
    fn test_parse_event_data_10byte() {
        let mut data = Vec::new();
        data.write_u16::<LittleEndian>(101).unwrap(); // uid
        data.push(1); // status
        data.push(0); // punch
        data.extend_from_slice(&[26, 2, 20, 10, 30, 0]); // timehex: 2026-02-20 10:30:00

        let event = parse_event_data(&data).unwrap();
        assert_eq!(event.uid, 101);
        assert_eq!(event.user_id, "101");
        assert_eq!(event.status, 1);
        assert_eq!(event.punch, 0);
        assert_eq!(event.timestamp.to_string(), "2026-02-20 10:30:00");
    }

    #[test]
    fn test_parse_event_data_12byte() {
        let mut data = Vec::new();
        data.write_u32::<LittleEndian>(9999).unwrap(); // user_id_num
        data.push(2); // status
        data.push(3); // punch
        data.extend_from_slice(&[26, 2, 20, 15, 0, 0]); // timehex: 2026-02-20 15:00:00

        let event = parse_event_data(&data).unwrap();
        assert_eq!(event.uid, 9999);
        assert_eq!(event.user_id, "9999");
        assert_eq!(event.status, 2);
        assert_eq!(event.punch, 3);
        assert_eq!(event.timestamp.to_string(), "2026-02-20 15:00:00");
    }

    #[test]
    fn test_parse_event_data_32byte() {
        let mut data = vec![0u8; 32];
        data[..13].copy_from_slice(b"RUST-USER-001");
        data[24] = 1; // status
        data[25] = 0; // punch
        data[26..32].copy_from_slice(&[26, 2, 20, 15, 0, 0]); // timehex

        let event = parse_event_data(&data).unwrap();
        assert_eq!(event.uid, 0);
        assert_eq!(event.user_id, "RUST-USER-001");
        assert_eq!(event.status, 1);
        assert_eq!(event.punch, 0);
        assert_eq!(event.timestamp.to_string(), "2026-02-20 15:00:00");
    }

    #[test]
    fn test_parse_event_data_unknown_length() {
        // 5 bytes doesn't match any known format
        let data = vec![0u8; 5];
        let err = parse_event_data(&data).unwrap_err();
        assert!(matches!(
            err,
            ZKError::InvalidData(ZKErrorCode::InvalidDataFormat, _)
        ));
    }

    #[test]
    fn test_parse_event_data_invalid_timehex() {
        // 10-byte format with invalid timehex (all zeros = year 2000, month 0 — invalid)
        let mut data = Vec::new();
        data.write_u16::<LittleEndian>(1).unwrap();
        data.push(0);
        data.push(0);
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // month=0 is invalid

        let err = parse_event_data(&data).unwrap_err();
        assert!(matches!(
            err,
            ZKError::InvalidData(ZKErrorCode::InvalidDataFormat, _)
        ));
    }

    // ── parse_records_with tests ───────────────────────────────────────

    /// Stub: create a minimal 8-byte attendance record for parse_records_with tests.
    fn make_stub_record(uid: u16, status: u8, punch: u8) -> Vec<u8> {
        use byteorder::LittleEndian;
        let mut buf = Vec::with_capacity(8);
        buf.write_u16::<LittleEndian>(uid).unwrap();
        buf.push(status);
        // Time bytes: encode a valid timestamp (2025-01-01 00:00:00)
        // encode_time((25*12*31 + 0*31 + 0)*86400 + 0) = (9300)*86400 = 803520000
        let t: u32 = 803520000;
        buf.write_u32::<LittleEndian>(t).unwrap();
        buf.push(punch);
        buf
    }

    #[test]
    fn test_parse_records_with_single_record() {
        let data = make_stub_record(101, 1, 0);
        let mut out = Vec::new();
        parse_records_with(&data, 8, 8, &mut out, |chunk| {
            let mut rdr = std::io::Cursor::new(chunk);
            let uid = rdr.read_u16::<byteorder::LittleEndian>().unwrap() as u32;
            Ok(Attendance::new(
                uid,
                uid.to_string(),
                chrono::NaiveDate::from_ymd_opt(2025, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                1,
                0,
                420,
            ))
        })
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].uid(), 101);
    }

    #[test]
    fn test_parse_records_with_record_size_larger_than_chunk() {
        // Simulate records with padding: record_size=10 but chunk_size=8
        let rec1 = make_stub_record(1, 1, 0);
        let rec2 = make_stub_record(2, 1, 0);
        // Add 2 bytes padding per record
        let mut data = Vec::new();
        data.extend_from_slice(&rec1);
        data.extend_from_slice(&[0, 0]); // padding
        data.extend_from_slice(&rec2);
        data.extend_from_slice(&[0, 0]); // padding
        let mut out = Vec::new();
        parse_records_with(&data, 10, 8, &mut out, |chunk| {
            let uid = chunk[0] as u32 + ((chunk[1] as u32) << 8);
            Ok(Attendance::new(
                uid,
                uid.to_string(),
                chrono::NaiveDate::from_ymd_opt(2025, 1, 1)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                1,
                0,
                420,
            ))
        })
        .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].uid(), 1);
        assert_eq!(out[1].uid(), 2);
    }

    #[test]
    fn test_parse_records_with_empty_data() {
        let mut out = Vec::new();
        parse_records_with(&[], 8, 8, &mut out, |_chunk| unreachable!()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn test_parse_records_with_data_smaller_than_chunk() {
        // Data smaller than chunk_size → no iterations
        let mut out = Vec::new();
        parse_records_with(&[1, 2, 3], 8, 8, &mut out, |_chunk| unreachable!()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn test_parse_records_with_propagates_error() {
        let data = make_stub_record(1, 1, 0);
        let mut out = Vec::new();
        let err = parse_records_with(&data, 8, 8, &mut out, |_chunk| {
            Err(ZKError::Response(ZKErrorCode::Other, "test error".into()))
        })
        .unwrap_err();
        assert!(matches!(err, ZKError::Response(ZKErrorCode::Other, _)));
    }
}
