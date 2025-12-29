//! Improv WiFi protocol implementation.
//!
//! See https://www.improv-wifi.com/ble/ for the protocol specification.

use bluer::Uuid;

/// Improv WiFi service UUID.
pub const SERVICE_UUID: Uuid = Uuid::from_u128(0x00467768_6228_2272_4663_277478268000);

/// Characteristic UUIDs.
pub mod characteristic {
    use bluer::Uuid;

    /// Current State characteristic - reports provisioning state.
    pub const CURRENT_STATE: Uuid = Uuid::from_u128(0x00467768_6228_2272_4663_277478268001);

    /// Error State characteristic - reports error conditions.
    pub const ERROR_STATE: Uuid = Uuid::from_u128(0x00467768_6228_2272_4663_277478268002);

    /// RPC Command characteristic - receives commands from client.
    pub const RPC_COMMAND: Uuid = Uuid::from_u128(0x00467768_6228_2272_4663_277478268003);

    /// RPC Result characteristic - sends responses to client.
    pub const RPC_RESULT: Uuid = Uuid::from_u128(0x00467768_6228_2272_4663_277478268004);

    /// Capabilities characteristic - reports supported features.
    pub const CAPABILITIES: Uuid = Uuid::from_u128(0x00467768_6228_2272_4663_277478268005);
}

/// Improv device state values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImprovState {
    /// Device requires authorization before accepting commands.
    AuthorizationRequired = 0x01,
    /// Device is authorized and ready to receive credentials.
    Authorized = 0x02,
    /// Device is attempting to connect to WiFi.
    Provisioning = 0x03,
    /// Device has successfully connected to WiFi.
    Provisioned = 0x04,
}

impl From<ImprovState> for u8 {
    fn from(state: ImprovState) -> u8 {
        state as u8
    }
}

/// Improv error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ImprovError {
    /// No error.
    None = 0x00,
    /// Invalid RPC packet received.
    InvalidRpc = 0x01,
    /// Unknown RPC command.
    UnknownCommand = 0x02,
    /// Unable to connect to WiFi network.
    UnableToConnect = 0x03,
    /// Not authorized to perform this action.
    NotAuthorized = 0x04,
    /// Bad hostname provided.
    BadHostname = 0x05,
    /// Unknown error occurred.
    Unknown = 0xFF,
}

impl From<ImprovError> for u8 {
    fn from(error: ImprovError) -> u8 {
        error as u8
    }
}

/// Capability flags.
pub mod capabilities {
    /// Device can identify itself (e.g., blink LED).
    pub const IDENTIFY: u8 = 0x01;
}

/// RPC command IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RpcCommand {
    /// Send WiFi credentials (SSID + password).
    SendWifiSettings = 0x01,
    /// Request device identification (blink LED, etc.).
    Identify = 0x02,
    /// Get device information.
    GetDeviceInfo = 0x03,
    /// Scan for WiFi networks.
    ScanWifiNetworks = 0x04,
    /// Get or set hostname.
    Hostname = 0x05,
}

impl TryFrom<u8> for RpcCommand {
    type Error = RpcError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(RpcCommand::SendWifiSettings),
            0x02 => Ok(RpcCommand::Identify),
            0x03 => Ok(RpcCommand::GetDeviceInfo),
            0x04 => Ok(RpcCommand::ScanWifiNetworks),
            0x05 => Ok(RpcCommand::Hostname),
            _ => Err(RpcError::UnknownCommand(value)),
        }
    }
}

/// Error parsing RPC packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcError {
    /// Packet too short.
    TooShort,
    /// Invalid checksum.
    BadChecksum { expected: u8, actual: u8 },
    /// Unknown command ID.
    UnknownCommand(u8),
    /// Data length doesn't match packet size.
    LengthMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::TooShort => write!(f, "Packet too short"),
            RpcError::BadChecksum { expected, actual } => {
                write!(f, "Bad checksum: expected {:#04x}, got {:#04x}", expected, actual)
            }
            RpcError::UnknownCommand(cmd) => write!(f, "Unknown command: {:#04x}", cmd),
            RpcError::LengthMismatch { expected, actual } => {
                write!(f, "Length mismatch: expected {}, got {}", expected, actual)
            }
        }
    }
}

impl std::error::Error for RpcError {}

/// Parsed RPC request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcRequest {
    pub command: RpcCommand,
    pub data: Vec<u8>,
}

/// WiFi credentials parsed from SendWifiSettings command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiCredentials {
    pub ssid: String,
    pub password: String,
}

impl RpcRequest {
    /// Parse an RPC packet from raw bytes.
    ///
    /// Packet format:
    /// - Byte 0: Command ID
    /// - Byte 1: Data length
    /// - Bytes 2..2+len: Data
    /// - Final byte: Checksum (sum of all preceding bytes, LSB only)
    pub fn parse(data: &[u8]) -> Result<Self, RpcError> {
        // Minimum packet: command + length + checksum = 3 bytes.
        if data.len() < 3 {
            return Err(RpcError::TooShort);
        }

        let command_byte = data[0];
        let data_len = data[1] as usize;

        // Check total length: command + length + data + checksum.
        let expected_len = 2 + data_len + 1;
        if data.len() != expected_len {
            return Err(RpcError::LengthMismatch {
                expected: expected_len,
                actual: data.len(),
            });
        }

        // Verify checksum.
        let checksum_idx = data.len() - 1;
        let expected_checksum = calculate_checksum(&data[..checksum_idx]);
        let actual_checksum = data[checksum_idx];

        if expected_checksum != actual_checksum {
            return Err(RpcError::BadChecksum {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }

        // Parse command.
        let command = RpcCommand::try_from(command_byte)?;

        // Extract data.
        let payload = data[2..2 + data_len].to_vec();

        Ok(RpcRequest {
            command,
            data: payload,
        })
    }

    /// Parse WiFi credentials from a SendWifiSettings command.
    ///
    /// Data format:
    /// - Byte 0: SSID length
    /// - Bytes 1..1+ssid_len: SSID
    /// - Next byte: Password length
    /// - Following bytes: Password
    pub fn parse_wifi_credentials(&self) -> Result<WifiCredentials, RpcError> {
        if self.command != RpcCommand::SendWifiSettings {
            return Err(RpcError::UnknownCommand(self.command as u8));
        }

        if self.data.is_empty() {
            return Err(RpcError::TooShort);
        }

        let ssid_len = self.data[0] as usize;
        if self.data.len() < 1 + ssid_len + 1 {
            return Err(RpcError::TooShort);
        }

        let ssid = String::from_utf8_lossy(&self.data[1..1 + ssid_len]).to_string();

        let password_len = self.data[1 + ssid_len] as usize;
        let password_start = 2 + ssid_len;

        if self.data.len() < password_start + password_len {
            return Err(RpcError::TooShort);
        }

        let password =
            String::from_utf8_lossy(&self.data[password_start..password_start + password_len])
                .to_string();

        Ok(WifiCredentials { ssid, password })
    }
}

/// Calculate checksum for a byte slice.
///
/// The checksum is the sum of all bytes, keeping only the LSB.
pub fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Build an RPC response packet.
///
/// Response format:
/// - Byte 0: Command that was executed
/// - Byte 1: Total data length
/// - Bytes 2+: String list (each string prefixed with length byte)
/// - Final byte: Checksum
pub fn build_response(command: RpcCommand, strings: &[&str]) -> Vec<u8> {
    let mut packet = Vec::new();

    // Command byte.
    packet.push(command as u8);

    // Calculate total data length (sum of length bytes + string bytes).
    let data_len: usize = strings.iter().map(|s| 1 + s.len()).sum();
    packet.push(data_len as u8);

    // Add each string with its length prefix.
    for s in strings {
        packet.push(s.len() as u8);
        packet.extend_from_slice(s.as_bytes());
    }

    // Add checksum.
    let checksum = calculate_checksum(&packet);
    packet.push(checksum);

    packet
}

/// Build a device info response.
///
/// Returns firmware name, version, hardware type, and device name.
pub fn build_device_info_response(
    firmware_name: &str,
    firmware_version: &str,
    hardware_type: &str,
    device_name: &str,
) -> Vec<u8> {
    build_response(
        RpcCommand::GetDeviceInfo,
        &[firmware_name, firmware_version, hardware_type, device_name],
    )
}

/// Build a WiFi scan result response.
///
/// Each network is: "SSID,RSSI,AUTH" (AUTH is 1 if secured, 0 if open).
pub fn build_scan_response(networks: &[(String, i32, bool)]) -> Vec<u8> {
    let network_strings: Vec<String> = networks
        .iter()
        .map(|(ssid, rssi, secured)| format!("{},{},{}", ssid, rssi, if *secured { 1 } else { 0 }))
        .collect();

    let str_refs: Vec<&str> = network_strings.iter().map(|s| s.as_str()).collect();
    build_response(RpcCommand::ScanWifiNetworks, &str_refs)
}

/// Build a successful provisioning response with redirect URL.
pub fn build_provision_response(redirect_url: &str) -> Vec<u8> {
    build_response(RpcCommand::SendWifiSettings, &[redirect_url])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checksum() {
        // Simple case.
        assert_eq!(calculate_checksum(&[1, 2, 3]), 6);

        // Overflow wraps.
        assert_eq!(calculate_checksum(&[255, 1]), 0);
        assert_eq!(calculate_checksum(&[255, 2]), 1);

        // Empty.
        assert_eq!(calculate_checksum(&[]), 0);
    }

    #[test]
    fn test_parse_identify_command() {
        // Identify command with no data: [cmd=0x02, len=0x00, checksum].
        let checksum = calculate_checksum(&[0x02, 0x00]);
        let packet = vec![0x02, 0x00, checksum];

        let request = RpcRequest::parse(&packet).unwrap();
        assert_eq!(request.command, RpcCommand::Identify);
        assert!(request.data.is_empty());
    }

    #[test]
    fn test_parse_get_device_info() {
        let checksum = calculate_checksum(&[0x03, 0x00]);
        let packet = vec![0x03, 0x00, checksum];

        let request = RpcRequest::parse(&packet).unwrap();
        assert_eq!(request.command, RpcCommand::GetDeviceInfo);
    }

    #[test]
    fn test_parse_wifi_credentials() {
        // SendWifiSettings with SSID="test" and password="pass".
        let ssid = b"test";
        let password = b"pass";

        let mut data = Vec::new();
        data.push(0x01); // Command: SendWifiSettings.
        data.push((1 + ssid.len() + 1 + password.len()) as u8); // Data length.
        data.push(ssid.len() as u8);
        data.extend_from_slice(ssid);
        data.push(password.len() as u8);
        data.extend_from_slice(password);

        let checksum = calculate_checksum(&data);
        data.push(checksum);

        let request = RpcRequest::parse(&data).unwrap();
        assert_eq!(request.command, RpcCommand::SendWifiSettings);

        let creds = request.parse_wifi_credentials().unwrap();
        assert_eq!(creds.ssid, "test");
        assert_eq!(creds.password, "pass");
    }

    #[test]
    fn test_parse_bad_checksum() {
        let packet = vec![0x02, 0x00, 0xFF]; // Wrong checksum.
        let result = RpcRequest::parse(&packet);

        match result {
            Err(RpcError::BadChecksum { .. }) => {}
            _ => panic!("Expected BadChecksum error"),
        }
    }

    #[test]
    fn test_parse_too_short() {
        let result = RpcRequest::parse(&[0x02]);
        assert!(matches!(result, Err(RpcError::TooShort)));
    }

    #[test]
    fn test_parse_length_mismatch() {
        // Says data length is 5, but only 0 bytes of data provided.
        let packet = vec![0x02, 0x05, 0x07];
        let result = RpcRequest::parse(&packet);

        match result {
            Err(RpcError::LengthMismatch { .. }) => {}
            _ => panic!("Expected LengthMismatch error"),
        }
    }

    #[test]
    fn test_parse_unknown_command() {
        let checksum = calculate_checksum(&[0xFF, 0x00]);
        let packet = vec![0xFF, 0x00, checksum];
        let result = RpcRequest::parse(&packet);

        match result {
            Err(RpcError::UnknownCommand(0xFF)) => {}
            _ => panic!("Expected UnknownCommand error"),
        }
    }

    #[test]
    fn test_build_device_info_response() {
        let response = build_device_info_response("wifi-provisioner", "0.1.0", "Pi", "DirtSim");

        // Verify structure: cmd + len + strings + checksum.
        assert_eq!(response[0], RpcCommand::GetDeviceInfo as u8);

        // Verify checksum.
        let checksum_idx = response.len() - 1;
        let expected_checksum = calculate_checksum(&response[..checksum_idx]);
        assert_eq!(response[checksum_idx], expected_checksum);
    }

    #[test]
    fn test_build_response_roundtrip() {
        let original = build_response(RpcCommand::Identify, &[]);

        // Should be parseable (though Identify doesn't normally have a response).
        assert_eq!(original[0], 0x02);
        assert_eq!(original[1], 0x00); // No data.

        let checksum = calculate_checksum(&original[..original.len() - 1]);
        assert_eq!(original[original.len() - 1], checksum);
    }

    #[test]
    fn test_service_uuid() {
        // Verify UUID format matches spec.
        let uuid_str = SERVICE_UUID.to_string();
        assert_eq!(uuid_str, "00467768-6228-2272-4663-277478268000");
    }

    #[test]
    fn test_characteristic_uuids() {
        assert_eq!(
            characteristic::CURRENT_STATE.to_string(),
            "00467768-6228-2272-4663-277478268001"
        );
        assert_eq!(
            characteristic::RPC_COMMAND.to_string(),
            "00467768-6228-2272-4663-277478268003"
        );
    }
}
