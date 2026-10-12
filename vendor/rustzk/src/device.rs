use byteorder::ByteOrder;
use chrono::{DateTime, FixedOffset, TimeZone};

use crate::constants::*;
use crate::models::DeviceInfo;
use crate::{ZKError, ZKErrorCode, ZKResult, ZK};

/// Read a 4-byte little-endian i32 from `data` at the given byte offset.
#[inline]
fn read_sizes_field(data: &[u8], offset: usize) -> i32 {
    byteorder::LittleEndian::read_i32(&data[offset..offset + SIZES_FIELD_LEN])
}

impl ZK {
    /// Fetches device capacity and usage statistics.
    pub fn read_sizes(&mut self) -> ZKResult<()> {
        let mut res = self.send_command(CMD_GET_FREE_SIZES, &[])?;

        // Handle case where device sends ACK_OK then ACK_DATA/Response separately
        if res.command() == CMD_ACK_OK && res.payload().len() < SIZES_ACK_FALLBACK_MIN {
            // Try reading the next packet which should contain the actual data
            match self.read_response_safe() {
                Ok(next_packet) => {
                    res = next_packet;
                }
                Err(e) => {
                    log::debug!(
                        "read_sizes: received ACK_OK but failed to read subsequent data: {}",
                        e
                    );
                }
            }
        }

        if res.command() == CMD_ACK_OK || res.command() == CMD_ACK_DATA {
            let data = res.payload();
            let mut info = DeviceInfo::default();
            if data.len() >= SIZES_V2_MIN {
                info.users = read_sizes_field(data, SIZES_V2_USERS).max(0) as u32;
                info.fingers = read_sizes_field(data, SIZES_V2_FINGERS).max(0) as u32;
                info.records = read_sizes_field(data, SIZES_V2_RECORDS).max(0) as u32;
                info.cards = read_sizes_field(data, SIZES_V2_CARDS).max(0) as u32;
                info.fingers_cap = read_sizes_field(data, SIZES_V2_FINGERS_CAP).max(0) as u32;
                info.users_cap = read_sizes_field(data, SIZES_V2_USERS_CAP).max(0) as u32;
                info.rec_cap = read_sizes_field(data, SIZES_V2_REC_CAP).max(0) as u32;

                if data.len() >= SIZES_V2_EXT_MIN {
                    info.faces = read_sizes_field(data, SIZES_V2_FACES).max(0) as u32;
                    info.faces_cap = read_sizes_field(data, SIZES_V2_FACES_CAP).max(0) as u32;
                }
            } else if data.len() >= SIZES_V1_MIN {
                // Older firmware formats
                info.users = read_sizes_field(data, SIZES_V1_USERS).max(0) as u32;
                info.fingers = read_sizes_field(data, SIZES_V1_FINGERS).max(0) as u32;
                info.records = read_sizes_field(data, SIZES_V1_RECORDS).max(0) as u32;
                info.users_cap = read_sizes_field(data, SIZES_V1_USERS_CAP).max(0) as u32;
                info.fingers_cap = read_sizes_field(data, SIZES_V1_FINGERS_CAP).max(0) as u32;
                info.rec_cap = read_sizes_field(data, SIZES_V1_REC_CAP).max(0) as u32;
            }
            self.device_info = Some(info);
            let _ = self.sync_timezone();
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                format!(
                    "Failed to read sizes (Device returned CMD 0x{:X})",
                    res.command()
                ),
            ))
        }
    }

    pub fn get_firmware_version(&mut self) -> ZKResult<String> {
        let res = self.send_command(CMD_GET_VERSION, &[])?;
        if res.command() == CMD_ACK_OK || res.command() == CMD_ACK_DATA {
            Ok(String::from_utf8_lossy(res.payload())
                .trim_matches('\0')
                .to_string())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                "Can't read firmware version".into(),
            ))
        }
    }

    pub fn get_option_value(&mut self, key: &str) -> ZKResult<String> {
        let mut command_string = key.as_bytes().to_vec();
        command_string.push(0);
        let res = self.send_command(CMD_OPTIONS_RRQ, &command_string)?;
        if res.command() == CMD_ACK_OK || res.command() == CMD_ACK_DATA {
            let data = String::from_utf8_lossy(res.payload());
            let data_str = data.trim_matches('\0').to_string();

            // Usually returns "Key=Value"
            if let Some(pos) = data_str.find('=') {
                Ok(data_str[pos + 1..].to_string())
            } else {
                Ok(data_str)
            }
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                format!(
                    "Can't read option '{}' (Device returned CMD 0x{:X})",
                    key,
                    res.command()
                ),
            ))
        }
    }

    pub fn get_serial_number(&mut self) -> ZKResult<String> {
        self.get_option_value("~SerialNumber")
    }

    pub fn get_platform(&mut self) -> ZKResult<String> {
        self.get_option_value("~Platform")
    }

    /// Gets the device timezone adjustment (usually in hours).
    pub fn get_timezone(&mut self) -> ZKResult<i32> {
        let tz_str = self.get_option_value("TZAdj")?;
        tz_str.parse::<i32>().map_err(|_| {
            ZKError::InvalidData(
                ZKErrorCode::InvalidDataFormat,
                format!("Invalid timezone value: {}", tz_str),
            )
        })
    }

    pub fn get_mac(&mut self) -> ZKResult<String> {
        self.get_option_value("MAC")
    }

    pub fn get_device_name(&mut self) -> ZKResult<String> {
        self.get_option_value("~DeviceName")
    }

    pub fn get_face_version(&mut self) -> ZKResult<String> {
        self.get_option_value("ZKFaceVersion")
    }

    pub fn get_fp_version(&mut self) -> ZKResult<String> {
        self.get_option_value("~ZKFPVersion")
    }

    /// Retrieves the current time from the device.
    ///
    /// **Note:** This returns the time as configured on the device, mapped to
    /// the detected timezone offset. If the device time falls into a DST
    /// transition gap (non-existent time), this will return an error.
    pub fn get_time(&mut self) -> ZKResult<DateTime<FixedOffset>> {
        let _ = self.sync_timezone();
        let res = self.send_command(CMD_GET_TIME, &[])?;
        if res.command() == CMD_ACK_OK || res.command() == CMD_ACK_DATA {
            let naive = ZK::decode_time(res.payload())?;
            let offset = FixedOffset::east_opt(self.timezone_offset * 60)
                .or_else(|| FixedOffset::east_opt(0))
                .ok_or_else(|| {
                    ZKError::InvalidData(
                        ZKErrorCode::InvalidDataFormat,
                        "Failed to construct timezone offset".into(),
                    )
                })?;

            match offset.from_local_datetime(&naive) {
                chrono::LocalResult::Single(dt) => Ok(dt),
                chrono::LocalResult::Ambiguous(dt1, _) => Ok(dt1),
                chrono::LocalResult::None => Err(ZKError::InvalidData(
                    ZKErrorCode::InvalidDataFormat,
                    "Invalid local datetime from device".into(),
                )),
            }
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                "Can't get time".into(),
            ))
        }
    }

    pub fn set_time(&mut self, t: DateTime<FixedOffset>) -> ZKResult<()> {
        // ZKTeco devices usually work in local time.
        let local_naive = t.naive_local();
        let encoded = ZK::encode_time(local_naive);
        let mut payload = [0u8; 4];
        byteorder::LittleEndian::write_u32(&mut payload, encoded);

        let res = self.send_command(CMD_SET_TIME, &payload)?;
        if res.command() == CMD_ACK_OK {
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                "Failed to set time".into(),
            ))
        }
    }

    /// Sets a device option by key and value.
    pub fn set_option(&mut self, key: &str, value: &str) -> ZKResult<()> {
        let command_string = format!("{}={}\0", key, value);
        let res = self.send_command(CMD_OPTIONS_WRQ, command_string.as_bytes())?;
        if res.command() == CMD_ACK_OK {
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                format!("Failed to set option {}", key),
            ))
        }
    }

    /// Changes the communication password (CommKey) of the device.
    /// Note: After changing the password, you must use the new password for future connections.
    pub fn change_password(&mut self, new_password: u32) -> ZKResult<()> {
        self.set_option("ComKey", &new_password.to_string())?;
        self.config.password = new_password; // Update local state to match
        Ok(())
    }

    pub fn restart(&mut self) -> ZKResult<()> {
        let result = self.send_command(CMD_RESTART, &[]);
        self.connection.is_connected = false;
        self.connection.transport = None;
        result.map(|_| ())
    }

    pub fn poweroff(&mut self) -> ZKResult<()> {
        let result = self.send_command(CMD_POWEROFF, &[]);
        self.connection.is_connected = false;
        self.connection.transport = None;
        result.map(|_| ())
    }

    pub fn unlock(&mut self, seconds: u32) -> ZKResult<()> {
        let mut payload = [0u8; 4];
        byteorder::LittleEndian::write_u32(&mut payload, seconds * 10);
        let res = self.send_command(CMD_UNLOCK, &payload)?;
        if res.command() == CMD_ACK_OK {
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                "Can't open door".into(),
            ))
        }
    }

    /// Refreshes the device's internal data.
    pub fn refresh_data(&mut self) -> ZKResult<()> {
        let res = self.send_command(CMD_REFRESHDATA, &[])?;
        if res.command() == CMD_ACK_OK {
            Ok(())
        } else {
            Err(ZKError::Response(
                ZKErrorCode::ProtocolViolation,
                "Failed to refresh data".into(),
            ))
        }
    }

    /// Returns the detected timezone offset in minutes.
    pub fn timezone_offset(&self) -> i32 {
        self.timezone_offset
    }

    /// Returns true if the timezone has been synchronized with the device.
    pub fn timezone_synced(&self) -> bool {
        self.timezone_synced
    }

    pub fn users(&self) -> u32 {
        self.device_info.map_or(0, |d| d.users)
    }
    pub fn users_cap(&self) -> u32 {
        self.device_info.map_or(0, |d| d.users_cap)
    }
    pub fn fingers(&self) -> u32 {
        self.device_info.map_or(0, |d| d.fingers)
    }
    pub fn fingers_cap(&self) -> u32 {
        self.device_info.map_or(0, |d| d.fingers_cap)
    }
    pub fn records(&self) -> u32 {
        self.device_info.map_or(0, |d| d.records)
    }
    pub fn records_cap(&self) -> u32 {
        self.device_info.map_or(0, |d| d.rec_cap)
    }
    pub fn faces(&self) -> u32 {
        self.device_info.map_or(0, |d| d.faces)
    }
    pub fn faces_cap(&self) -> u32 {
        self.device_info.map_or(0, |d| d.faces_cap)
    }
    pub fn cards(&self) -> u32 {
        self.device_info.map_or(0, |d| d.cards)
    }
    pub fn is_connected(&self) -> bool {
        self.connection.is_connected
    }
}
