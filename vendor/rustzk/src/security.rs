//! Security constants and utilities for rustzk
//!
//! This module provides security-focused constants and utilities
//! to ensure safe operation of the ZK protocol implementation.

/// Maximum allowed packet size for network operations
///
/// Default: 1MB (1,048,576 bytes)
/// Can be configured via RUSTZK_MAX_PACKET_SIZE environment variable
pub const MAX_PACKET_SIZE: usize = 1_048_576;

/// Legacy packet size limit for compatibility mode
///
/// Used only when explicitly enabled via feature flags
pub const LEGACY_PACKET_SIZE: usize = 16_777_216; // 16MB

/// Get the configured maximum packet size
///
/// Returns environment-configured size or default
pub fn get_max_packet_size() -> usize {
    std::env::var("RUSTZK_MAX_PACKET_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(MAX_PACKET_SIZE)
}

/// Validate packet size against security limits
///
/// # Arguments
/// * `size` - The packet size to validate
///
/// # Returns
/// * `Ok(())` if size is within limits
/// * `Err(ZKError::InvalidData)` if size exceeds limits
pub fn validate_packet_size(size: usize) -> crate::ZKResult<()> {
    let max_size = get_max_packet_size();
    if size > max_size {
        Err(crate::ZKError::InvalidData(
            crate::ZKErrorCode::BufferOverflow,
            format!(
                "Packet size {} exceeds maximum allowed size {}",
                size, max_size
            ),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_max_packet_size() {
        assert_eq!(get_max_packet_size(), MAX_PACKET_SIZE);
    }

    #[test]
    fn test_valid_packet_size() {
        assert!(validate_packet_size(1024).is_ok());
        assert!(validate_packet_size(MAX_PACKET_SIZE).is_ok());
    }

    #[test]
    fn test_invalid_packet_size() {
        assert!(validate_packet_size(MAX_PACKET_SIZE + 1).is_err());
    }

    #[test]
    fn test_environment_override() {
        std::env::set_var("RUSTZK_MAX_PACKET_SIZE", "2097152");
        assert_eq!(get_max_packet_size(), 2_097_152);
        std::env::remove_var("RUSTZK_MAX_PACKET_SIZE");
    }
}
