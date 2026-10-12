use byteorder::{ByteOrder, LittleEndian};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use crate::constants::*;
use crate::protocol::{TCPWrapper, ZKPacket};
use crate::{ZKError, ZKErrorCode, ZKProtocol, ZKResult, ZK};

pub enum ZKTransport {
    Tcp(std::io::BufReader<TcpStream>),
    Udp(UdpSocket),
}

impl ZK {
    pub(crate) fn set_transport_read_timeout(&mut self, timeout: Duration) {
        if let Some(ref mut transport) = self.connection.transport {
            match transport {
                ZKTransport::Tcp(reader) => {
                    let _ = reader.get_ref().set_read_timeout(Some(timeout));
                }
                ZKTransport::Udp(socket) => {
                    let _ = socket.set_read_timeout(Some(timeout));
                }
            }
        }
    }

    pub(crate) fn perform_connect_handshake(&mut self) -> ZKResult<()> {
        self.connection.session_id = 0;
        self.connection.reply_id = USHRT_MAX - 1;
        self.timezone_synced = false;
        self.timezone_offset = 0;

        // Use a short timeout for handshake probes (5s each) so auto-fallback
        // completes in ~10s max instead of waiting the full user-configured timeout.
        let handshake_timeout = Duration::from_secs(5);
        let saved_timeout = self.config.timeout;
        self.set_transport_read_timeout(handshake_timeout);

        // Attempt 1: Try current checksum algorithm
        let result = self.send_command(CMD_CONNECT, &[]);

        match result {
            Ok(res) => {
                // Restore original timeout for data transfers
                self.set_transport_read_timeout(saved_timeout);
                self.finish_handshake(res)
            }
            Err(ZKError::Network(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout means the device silently rejected our packet — wrong checksum.
                // Flip to the alternative checksum algorithm and retry.
                log::info!(
                    "Handshake timeout with {} checksum. Retrying with {} checksum...",
                    if self.connection.use_legacy_checksum {
                        "legacy"
                    } else {
                        "default"
                    },
                    if self.connection.use_legacy_checksum {
                        "default"
                    } else {
                        "legacy"
                    }
                );
                self.connection.use_legacy_checksum = !self.connection.use_legacy_checksum;
                self.connection.session_id = 0;
                self.connection.reply_id = USHRT_MAX - 1;

                // Attempt 2: Try alternative checksum algorithm
                let result = self.send_command(CMD_CONNECT, &[]);

                // Restore original timeout for data transfers
                self.set_transport_read_timeout(saved_timeout);

                match result {
                    Ok(res) => {
                        log::info!(
                            "Connected with {} checksum",
                            if self.connection.use_legacy_checksum {
                                "legacy"
                            } else {
                                "default"
                            }
                        );
                        self.finish_handshake(res)
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => {
                self.set_transport_read_timeout(saved_timeout);
                Err(e)
            }
        }
    }

    pub(crate) fn finish_handshake(&mut self, res: ZKPacket<'static>) -> ZKResult<()> {
        // Update session_id if we got a valid response (OK or UNAUTH)
        if res.command() == CMD_ACK_OK || res.command() == CMD_ACK_UNAUTH {
            self.connection.session_id = res.session_id();
        }

        if res.command() == CMD_ACK_UNAUTH {
            // Need authentication
            let command_string =
                ZK::make_commkey(self.config.password, self.connection.session_id, 50);
            let auth_res = self.send_command(CMD_AUTH, &command_string)?;
            if auth_res.command() == CMD_ACK_UNAUTH {
                self.connection.session_id = 0; // Reset dirty session_id on auth failure
                return Err(ZKError::Connection(
                    ZKErrorCode::Unauthorized,
                    "Unauthorized: Password required or incorrect".into(),
                ));
            }
            self.connection.session_id = auth_res.session_id();
            self.connection.is_connected = true;
            return Ok(());
        }

        if res.command() == CMD_ACK_OK {
            self.connection.is_connected = true;
            Ok(())
        } else {
            Err(ZKError::Connection(
                ZKErrorCode::ProtocolViolation,
                format!(
                    "Invalid response during connect handshake: {}",
                    res.command()
                ),
            ))
        }
    }

    pub(crate) fn sync_timezone(&mut self) -> ZKResult<()> {
        if self.timezone_synced {
            return Ok(());
        }

        // Mark as synced immediately to prevent repeated queries on failure
        self.timezone_synced = true;

        if let Ok(tz_str) = self.get_option_value("TZAdj") {
            if let Ok(tz_val) = tz_str.parse::<i32>() {
                self.timezone_offset = tz_val * 60; // Convert hours to minutes
            }
        }
        Ok(())
    }

    pub(crate) fn read_packet(&mut self) -> ZKResult<ZKPacket<'static>> {
        let transport = self.connection.transport.as_mut().ok_or_else(|| {
            ZKError::Connection(ZKErrorCode::ConnectionFailed, "Not connected".into())
        })?;

        let result = match transport {
            ZKTransport::Tcp(ref mut reader) => {
                let res = read_tcp_frame(reader);
                if let Err(ref e) = res {
                    if !e.is_timeout() {
                        log::warn!(
                            "TCP stream error (hard failure or desync). Closing transport: {:?}",
                            e
                        );
                        self.connection.is_connected = false;
                        self.connection.transport = None;
                    }
                }
                res
            }
            ZKTransport::Udp(ref mut socket) => {
                self.connection.udp_buf.resize(2048, 0);
                let len = socket.recv(&mut self.connection.udp_buf)?;
                let packet_data = self.connection.udp_buf[..len].to_vec();
                crate::security::validate_packet_size(packet_data.len())?;
                ZKPacket::from_bytes_owned(packet_data)
            }
        };

        let packet = result?;
        verify_packet_checksum(&packet, self.connection.use_legacy_checksum)?;
        Ok(packet)
    }

    pub(crate) fn read_response_safe(&mut self) -> ZKResult<ZKPacket<'static>> {
        let mut discarded = 0;
        loop {
            let res_packet = self.read_packet()?;
            log::debug!(
                "Received Packet: Cmd {} (0x{:X}), Reply ID: {}",
                res_packet.command(),
                res_packet.command(),
                res_packet.reply_id()
            );

            if res_packet.reply_id() != self.connection.reply_id {
                discarded += 1;
                log::debug!(
                    "Reply ID mismatch: expected {}, got {}. Discarding packet.",
                    self.connection.reply_id,
                    res_packet.reply_id()
                );
                if discarded > MAX_DISCARDED_PACKETS {
                    return Err(ZKError::Response(
                        ZKErrorCode::ProtocolViolation,
                        "Too many discarded packets".into(),
                    ));
                }
                continue;
            }
            return Ok(res_packet);
        }
    }

    /// Send a ZKPacket over the current transport.
    /// Handles TCP framing (magic + length prefix) and UDP direct send uniformly.
    pub(crate) fn send_packet(&mut self, packet: &ZKPacket<'_>) -> ZKResult<()> {
        let transport = self.connection.transport.as_mut().ok_or_else(|| {
            ZKError::Connection(ZKErrorCode::ConnectionFailed, "Not connected".into())
        })?;

        match transport {
            ZKTransport::Tcp(ref mut reader) => {
                self.connection.write_buf.clear();
                packet.to_bytes_into(&mut self.connection.write_buf)?;
                let framed = TCPWrapper::wrap(&self.connection.write_buf);
                reader.get_mut().write_all(&framed)?;
            }
            ZKTransport::Udp(ref mut socket) => {
                self.connection.write_buf.clear();
                packet.to_bytes_into(&mut self.connection.write_buf)?;
                socket.send(&self.connection.write_buf)?;
            }
        }
        Ok(())
    }

    /// Increment and wrap the reply ID for the next command-response cycle.
    fn increment_reply_id(&mut self) {
        self.connection.reply_id = self.connection.reply_id.wrapping_add(1);
        if self.connection.reply_id == USHRT_MAX {
            self.connection.reply_id -= USHRT_MAX;
        }
    }

    pub(crate) fn send_command(
        &mut self,
        command: u16,
        payload: &[u8],
    ) -> ZKResult<ZKPacket<'static>> {
        if self.connection.transport.is_none() {
            return Err(ZKError::Connection(
                ZKErrorCode::ConnectionFailed,
                "Not connected".into(),
            ));
        }

        self.increment_reply_id();

        log::debug!(
            "Sending Command: {} (0x{:X}), Reply ID: {}",
            command,
            command,
            self.connection.reply_id
        );

        let packet = if self.connection.use_legacy_checksum {
            ZKPacket::new_with_legacy(
                command,
                self.connection.session_id,
                self.connection.reply_id,
                payload,
            )
        } else {
            ZKPacket::new(
                command,
                self.connection.session_id,
                self.connection.reply_id,
                payload,
            )
        };

        self.send_packet(&packet)?;

        self.read_response_safe()
    }

    fn receive_chunk_into(&mut self, res: ZKPacket<'static>, data: &mut Vec<u8>) -> ZKResult<()> {
        if res.command() == CMD_DATA {
            data.extend_from_slice(res.payload());
            Ok(())
        } else if res.command() == CMD_ACK_OK {
            // New firmware may send ACK_OK before actual data.
            // Give the device a little time to prepare.
            std::thread::sleep(std::time::Duration::from_millis(10));
            Ok(())
        } else if res.command() == CMD_PREPARE_DATA {
            if res.payload().len() < 4 {
                return Err(ZKError::InvalidData(
                    ZKErrorCode::InvalidDataFormat,
                    "Invalid prepare data payload".into(),
                ));
            }
            let size = byteorder::LittleEndian::read_u32(&res.payload()[..4]) as usize;

            if size > MAX_RESPONSE_SIZE {
                return Err(ZKError::InvalidData(
                    ZKErrorCode::BufferOverflow,
                    format!(
                        "Response size {} exceeds maximum {}",
                        size, MAX_RESPONSE_SIZE
                    ),
                ));
            }

            data.reserve(size);
            let mut remaining = size;

            while remaining > 0 {
                let chunk_res = self.read_response_safe()?;

                if chunk_res.command() == CMD_DATA {
                    data.extend_from_slice(chunk_res.payload());
                    if remaining >= chunk_res.payload().len() {
                        remaining -= chunk_res.payload().len();
                    } else {
                        remaining = 0;
                    }
                } else if chunk_res.command() == CMD_ACK_OK {
                    break;
                } else {
                    return Err(ZKError::Response(
                        ZKErrorCode::ProtocolViolation,
                        format!("Unexpected chunk command: 0x{:X}", chunk_res.command()),
                    ));
                }
            }
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                format!("Invalid response for chunk: 0x{:X}", res.command()),
            ))
        }
    }

    fn read_chunk_into(&mut self, start: i32, size: i32, data: &mut Vec<u8>) -> ZKResult<()> {
        let is_udp = matches!(&self.connection.transport, Some(ZKTransport::Udp(_)));

        let mut payload = [0u8; 8];
        byteorder::LittleEndian::write_i32(&mut payload[0..4], start);
        byteorder::LittleEndian::write_i32(&mut payload[4..8], size);

        let max_attempts = if is_udp { 3 } else { 1 };
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            let res = self.send_command(_CMD_READ_BUFFER, &payload);
            match res {
                Ok(res_packet) => {
                    let receive_res = if res_packet.command() == CMD_ACK_OK {
                        // If we get ACK for read chunk, it means data is coming next.
                        // Wait for the actual data packet.
                        match self.read_response_safe() {
                            Ok(data_packet) => self.receive_chunk_into(data_packet, data),
                            Err(e) => Err(e),
                        }
                    } else {
                        self.receive_chunk_into(res_packet, data)
                    };

                    match receive_res {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            log::warn!("UDP chunk receive failed on attempt {}: {:?}", attempt, e);
                            last_error = Some(e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("UDP chunk request failed on attempt {}: {:?}", attempt, e);
                    last_error = Some(e);
                }
            }

            if attempt < max_attempts {
                // Short wait before retry (exponential backoff)
                std::thread::sleep(std::time::Duration::from_millis(attempt as u64 * 100));
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ZKError::Response(ZKErrorCode::ProtocolViolation, "Chunk read failed".into())
        }))
    }

    /// Drain a buffered response in chunks, handling empty-response backoff.
    fn drain_buffer_chunks(&mut self, mut size: usize, max_chunk: usize) -> ZKResult<Vec<u8>> {
        let mut data = Vec::with_capacity(size);
        let mut start: usize = 0;
        let mut tracker = EmptyResponseTracker::new();

        while size > 0 {
            let chunk_size = std::cmp::min(size, max_chunk);
            let len_before = data.len();
            self.read_chunk_into(start as i32, chunk_size as i32, &mut data)?;
            let chunk_len = data.len() - len_before;

            if chunk_len == 0 {
                tracker.record_empty()?;
                continue;
            }

            tracker.reset();
            start += chunk_len;
            size = size.saturating_sub(chunk_len);
        }
        Ok(data)
    }

    pub(crate) fn read_with_buffer(
        &mut self,
        command: u16,
        fct: u32,
        size: u32,
    ) -> ZKResult<Vec<u8>> {
        let mut payload = [0u8; 11];
        payload[0] = 1;
        LittleEndian::write_u16(&mut payload[1..3], command);
        LittleEndian::write_u32(&mut payload[3..7], fct);
        LittleEndian::write_u32(&mut payload[7..11], size);

        let res = self.send_command(_CMD_PREPARE_BUFFER, &payload)?;
        if res.command() == CMD_DATA {
            return Ok(res.into_payload().into_owned());
        }

        let size = detect_buffered_response_size(res.payload())?;

        if size == 0 {
            let _ = self.send_command(CMD_FREE_DATA, &[]);
            return Ok(Vec::new());
        }

        let max_chunk = if matches!(&self.connection.transport, Some(ZKTransport::Tcp(_))) {
            TCP_MAX_CHUNK
        } else {
            UDP_MAX_CHUNK
        };

        let data = self.drain_buffer_chunks(size, max_chunk)?;
        let _ = self.send_command(CMD_FREE_DATA, &[]);
        Ok(data)
    }

    /// Upload a buffer to the device using the PREPARE_DATA / DATA handshake.
    ///
    /// This is the write-direction counterpart to [`read_with_buffer`]: the host
    /// announces the total size via `CMD_PREPARE_DATA`, then pushes the body in
    /// `CMD_DATA` chunks. The caller is responsible for issuing the subsequent
    /// commit command (e.g. `_CMD_SAVE_USERTEMPS`) that persists the staged data.
    ///
    /// Chunk size is fixed at 1024 bytes to match the reference firmware behaviour
    /// for template uploads (do not reuse the larger download chunk sizes).
    pub(crate) fn send_with_buffer(&mut self, buffer: &[u8]) -> ZKResult<()> {
        const UPLOAD_CHUNK: usize = 1024;

        // Clear any stale device buffer before staging new data.
        let _ = self.send_command(CMD_FREE_DATA, &[]);

        // Announce the total size.
        let mut size_payload = [0u8; 4];
        LittleEndian::write_u32(&mut size_payload, buffer.len() as u32);
        let res = self.send_command(CMD_PREPARE_DATA, &size_payload)?;
        if res.command() != CMD_ACK_OK {
            return Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                format!(
                    "Device rejected PREPARE_DATA for upload: 0x{:X}",
                    res.command()
                ),
            ));
        }

        // Stream the body in fixed-size chunks.
        for chunk in buffer.chunks(UPLOAD_CHUNK) {
            let res = self.send_command(CMD_DATA, chunk)?;
            if res.command() != CMD_ACK_OK {
                return Err(ZKError::Response(
                    ZKErrorCode::ProtocolViolation,
                    format!(
                        "Device rejected DATA chunk during upload: 0x{:X}",
                        res.command()
                    ),
                ));
            }
        }

        Ok(())
    }

    pub fn connect(&mut self, protocol: ZKProtocol) -> ZKResult<()> {
        if self.connection.is_connected {
            return Err(ZKError::Connection(
                ZKErrorCode::ConnectionFailed,
                "Already connected. Call disconnect() first.".into(),
            ));
        }
        match protocol {
            ZKProtocol::TCP => self.connect_tcp(),
            ZKProtocol::UDP => self.connect_udp(),
            ZKProtocol::Auto => {
                let saved_checksum = self.connection.use_legacy_checksum;
                match self.connect_tcp() {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        log::info!("TCP connect failed: {}. Falling back to UDP...", e);
                        self.connection.use_legacy_checksum = saved_checksum;
                        self.connect_udp()
                    }
                }
            }
        }
    }

    fn connect_tcp(&mut self) -> ZKResult<()> {
        let addrs = self.connection.addr.to_socket_addrs().map_err(|e| {
            ZKError::Connection(
                ZKErrorCode::ConnectionFailed,
                format!("Failed to resolve address {}: {}", self.connection.addr, e),
            )
        })?;
        let addr = addrs.into_iter().next().ok_or_else(|| {
            ZKError::Connection(
                ZKErrorCode::ConnectionFailed,
                format!("No address found for {}", self.connection.addr),
            )
        })?;

        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(self.config.timeout))?;
        stream.set_write_timeout(Some(self.config.timeout))?;

        self.connection.transport = Some(ZKTransport::Tcp(std::io::BufReader::new(stream)));
        match self.perform_connect_handshake() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.connection.transport = None;
                Err(e)
            }
        }
    }

    fn connect_udp(&mut self) -> ZKResult<()> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        let socket2_sock = socket2::Socket::from(socket);
        if let Err(e) = socket2_sock.set_recv_buffer_size(2 * 1024 * 1024) {
            log::debug!("Failed to set UDP receive buffer size (SO_RCVBUF): {}", e);
        }
        let socket = std::net::UdpSocket::from(socket2_sock);
        socket.connect(&self.connection.addr)?;
        socket.set_read_timeout(Some(self.config.timeout))?;
        socket.set_write_timeout(Some(self.config.timeout))?;

        self.connection.transport = Some(ZKTransport::Udp(socket));
        match self.perform_connect_handshake() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.connection.transport = None;
                Err(e)
            }
        }
    }

    pub(crate) fn send_exit_packet(&mut self) -> ZKResult<()> {
        if self.connection.transport.is_some() {
            self.connection.reply_id = self.connection.reply_id.wrapping_add(1);
            if self.connection.reply_id == USHRT_MAX {
                self.connection.reply_id -= USHRT_MAX;
            }

            let packet = if self.connection.use_legacy_checksum {
                ZKPacket::new_with_legacy(
                    CMD_EXIT,
                    self.connection.session_id,
                    self.connection.reply_id,
                    &[],
                )
            } else {
                ZKPacket::new(
                    CMD_EXIT,
                    self.connection.session_id,
                    self.connection.reply_id,
                    &[],
                )
            };

            self.send_packet(&packet)?;
        }
        Ok(())
    }

    pub fn disconnect(&mut self) -> ZKResult<()> {
        if self.connection.is_connected {
            self.set_transport_read_timeout(Duration::from_secs(3));
            let _ = self.send_command(CMD_EXIT, &[]);
            self.connection.is_connected = false;
        }
        self.connection.transport = None;
        Ok(())
    }
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// Tracks consecutive empty chunk responses during buffered reads,
/// applying exponential backoff (capped at 50 ms). Returns an error
/// after more than 20 consecutive empty responses to prevent infinite
/// spinning on a misbehaving device.
struct EmptyResponseTracker {
    count: u32,
}

impl EmptyResponseTracker {
    fn new() -> Self {
        Self { count: 0 }
    }

    /// Record an empty response. Returns `Err` once the limit is exceeded
    /// or sleeps with exponential backoff.
    fn record_empty(&mut self) -> ZKResult<()> {
        self.count += 1;
        if self.count > 20 {
            return Err(ZKError::Response(
                ZKErrorCode::Timeout,
                "Too many empty responses from device during buffer read".into(),
            ));
        }
        // Exponential backoff capped at 50 ms (1u64 << n is safe for n <= 20).
        let sleep_ms = std::cmp::min(1u64 << self.count, 50);
        std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
        Ok(())
    }

    fn reset(&mut self) {
        self.count = 0;
    }
}

/// Read and parse a complete TCP-framed ZK packet from the stream.
///
/// Handles partial header reads (desync detection), magic-number validation,
/// packet size bounds checking, and body deserialization.
fn read_tcp_frame(reader: &mut std::io::BufReader<TcpStream>) -> ZKResult<ZKPacket<'static>> {
    let mut header = [0u8; 8];
    // Read first chunk (up to 8 bytes). If this blocks/times out, 0 bytes
    // consumed — stream is still in sync.
    let n = match reader.read(&mut header[..]) {
        Ok(0) => {
            return Err(ZKError::Network(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "TCP connection closed (EOF)",
            )))
        }
        Ok(n) => n,
        Err(e) => return Err(ZKError::Network(e)),
    };

    // Partial read → read the rest. Any error here is a desync.
    if n < 8 {
        let mut rest = vec![0u8; 8 - n];
        if let Err(e) = reader.read_exact(&mut rest) {
            return Err(ZKError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("TCP desync: partial header read ({} bytes): {:?}", n, e),
            )));
        }
        header[n..8].copy_from_slice(&rest);
    }

    let (length, _) = TCPWrapper::decode_header(&header)
        .map_err(|e| ZKError::InvalidData(ZKErrorCode::InvalidDataFormat, e.to_string()))?;

    crate::security::validate_packet_size(length)?;

    let mut body = vec![0u8; length];
    // Any error reading the body is a desync (header already consumed).
    if let Err(e) = reader.read_exact(&mut body) {
        return Err(ZKError::Network(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "TCP desync: failed to read body of size {}: {:?}",
                length, e
            ),
        )));
    }

    ZKPacket::from_bytes_owned(body)
}

/// Verify the packet checksum using the appropriate algorithm.
fn verify_packet_checksum(packet: &ZKPacket, use_legacy: bool) -> ZKResult<()> {
    if !packet.verify_checksum(use_legacy) {
        return Err(ZKError::InvalidData(
            ZKErrorCode::ChecksumMismatch,
            "Invalid packet checksum".into(),
        ));
    }
    Ok(())
}

/// Decode the expected response size from a PREPARE_DATA response payload.
///
/// Handles firmware variants that report the size at different offsets:
/// - 5+ byte payloads: size at offset 1 (most common)
/// - 4-byte payloads: size at offset 0 (older firmware)
/// - Overflow fallback: tries alternative offset when primary exceeds limits
fn detect_buffered_response_size(payload: &[u8]) -> ZKResult<usize> {
    let mut size = if payload.len() >= 5 {
        byteorder::LittleEndian::read_u32(&payload[1..5]) as usize
    } else if payload.len() >= 4 {
        byteorder::LittleEndian::read_u32(&payload[0..4]) as usize
    } else {
        return Err(ZKError::Response(
            ZKErrorCode::InvalidDataFormat,
            format!("Invalid response size length: {}", payload.len()),
        ));
    };

    // Firmware quirk: some devices report size at offset 1 that exceeds limits,
    // but the correct size is at offset 0. Try alternative offset as fallback.
    if payload.len() >= 5 && size > MAX_RESPONSE_SIZE {
        let alt_size = byteorder::LittleEndian::read_u32(&payload[0..4]) as usize;
        if alt_size <= MAX_RESPONSE_SIZE {
            size = alt_size;
        }
    }

    if size > MAX_RESPONSE_SIZE {
        return Err(ZKError::InvalidData(
            ZKErrorCode::BufferOverflow,
            format!(
                "Buffered response size {} exceeds maximum {}",
                size, MAX_RESPONSE_SIZE
            ),
        ));
    }

    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── detect_buffered_response_size tests ────────────────────────────

    #[test]
    fn test_detect_size_5byte_payload_offset_1() {
        // Payload with flag byte at [0]=1, u32 size at [1..5]
        let mut payload = vec![0u8; 5];
        payload[0] = 1;
        byteorder::LittleEndian::write_u32(&mut payload[1..5], 1024);
        assert_eq!(detect_buffered_response_size(&payload).unwrap(), 1024);
    }

    #[test]
    fn test_detect_size_4byte_payload_offset_0() {
        // Older firmware: just 4 bytes with size at offset 0
        let mut payload = vec![0u8; 4];
        byteorder::LittleEndian::write_u32(&mut payload[0..4], 512);
        assert_eq!(detect_buffered_response_size(&payload).unwrap(), 512);
    }

    #[test]
    fn test_detect_size_too_short() {
        // Less than 4 bytes → error
        let err = detect_buffered_response_size(&[1, 2, 3]).unwrap_err();
        assert!(matches!(
            err,
            ZKError::Response(ZKErrorCode::InvalidDataFormat, _)
        ));
    }

    #[test]
    fn test_detect_size_firmware_quirk_fallback() {
        // Simulate a device where offset-1 gives a huge value (wrong interpretation)
        // but offset-0 gives the correct size.
        // Byte layout: [0x00, 0x08, 0x00, 0x00, 0x01]
        //   offset 1..5 → 0x01000008 = 16777224 > MAX_RESPONSE_SIZE
        //   offset 0..4 → 0x00000800 = 2048 ≤ MAX_RESPONSE_SIZE
        let payload = vec![0x00u8, 0x08, 0x00, 0x00, 0x01];
        assert_eq!(detect_buffered_response_size(&payload).unwrap(), 2048);
    }

    #[test]
    fn test_detect_size_both_offsets_exceed_max() {
        // Both offset-0 and offset-1 readings exceed MAX_RESPONSE_SIZE
        // Byte layout: all 0x01 → both offsets read 0x01010101 = 16843009 > 10485760
        let payload = vec![0x01u8, 0x01, 0x01, 0x01, 0x01];
        let err = detect_buffered_response_size(&payload).unwrap_err();
        assert!(matches!(
            err,
            ZKError::InvalidData(ZKErrorCode::BufferOverflow, _)
        ));
    }

    #[test]
    fn test_detect_size_zero() {
        let mut payload = vec![0u8; 5];
        payload[0] = 1;
        byteorder::LittleEndian::write_u32(&mut payload[1..5], 0);
        assert_eq!(detect_buffered_response_size(&payload).unwrap(), 0);
    }

    // ── verify_packet_checksum tests ───────────────────────────────────

    #[test]
    fn test_verify_checksum_valid() {
        let packet = ZKPacket::new(CMD_CONNECT, 0, 65534, vec![]);
        assert!(verify_packet_checksum(&packet, false).is_ok());
    }

    #[test]
    fn test_verify_checksum_invalid() {
        let packet = ZKPacket::new(CMD_CONNECT, 0, 65534, vec![]);
        // Verify with wrong algorithm → mismatch
        let err = verify_packet_checksum(&packet, true).unwrap_err();
        assert!(matches!(
            err,
            ZKError::InvalidData(ZKErrorCode::ChecksumMismatch, _)
        ));
    }

    #[test]
    fn test_verify_checksum_legacy_valid() {
        let packet = ZKPacket::new_with_legacy(CMD_CONNECT, 0, 65534, vec![]);
        assert!(verify_packet_checksum(&packet, true).is_ok());
    }

    // ── EmptyResponseTracker tests ──────────────────────────────────────

    #[test]
    fn test_empty_tracker_new_resets() {
        let mut tracker = EmptyResponseTracker::new();
        // One empty, reset, one more — should not hit limit
        assert!(tracker.record_empty().is_ok());
        tracker.reset();
        assert!(tracker.record_empty().is_ok());
    }

    #[test]
    fn test_empty_tracker_hits_limit() {
        let mut tracker = EmptyResponseTracker::new();
        for _ in 0..21 {
            let _ = tracker.record_empty();
        }
        let err = tracker.record_empty().unwrap_err();
        assert!(matches!(err, ZKError::Response(ZKErrorCode::Timeout, _)));
    }

    #[test]
    fn test_empty_tracker_ok_up_to_20() {
        let mut tracker = EmptyResponseTracker::new();
        // 20 empty responses are tolerated (limit is >20, meaning 21st call errors)
        for _ in 0..20 {
            assert!(tracker.record_empty().is_ok());
        }
        // The 21st call should fail
        assert!(tracker.record_empty().is_err());
    }
}
