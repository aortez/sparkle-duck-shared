//! WebSocket protocol types for wifi-provisioner.
//!
//! Defines the JSON command/response format for local IPC.

use serde::{Deserialize, Serialize};

/// Commands received from local clients (e.g., dirtsim UI).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Start BLE advertising with optional timeout in seconds.
    Start {
        #[serde(default = "default_timeout")]
        timeout: u32,
    },
    /// Stop BLE advertising.
    Stop,
    /// Get current daemon status.
    Status,
    /// Scan for available WiFi networks.
    Scan,
}

fn default_timeout() -> u32 {
    300 // 5 minutes.
}

/// Daemon state reported in responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Not advertising, waiting for trigger.
    Idle,
    /// BLE advertising active.
    Advertising,
    /// A BLE client is connected.
    Connected,
    /// WiFi provisioning in progress.
    Provisioning,
}

/// Response to a command.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Response {
    /// Successful response with state info.
    Ok(OkResponse),
    /// Error response.
    Error(ErrorResponse),
}

/// Successful response payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkResponse {
    pub ok: bool,
    pub state: State,
    /// Seconds remaining until advertising timeout (only when advertising).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u32>,
    /// Whether WiFi is currently connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_connected: Option<bool>,
    /// Available networks (only for scan response).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub networks: Option<Vec<Network>>,
}

impl OkResponse {
    /// Create a simple OK response with just state.
    pub fn new(state: State) -> Self {
        Self {
            ok: true,
            state,
            remaining: None,
            wifi_connected: None,
            networks: None,
        }
    }

    /// Add remaining time to response.
    pub fn with_remaining(mut self, secs: u32) -> Self {
        self.remaining = Some(secs);
        self
    }

    /// Add WiFi connection status.
    pub fn with_wifi_connected(mut self, connected: bool) -> Self {
        self.wifi_connected = Some(connected);
        self
    }

    /// Add network list.
    pub fn with_networks(mut self, networks: Vec<Network>) -> Self {
        self.networks = Some(networks);
        self
    }
}

/// Error response payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: String,
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: message.into(),
        }
    }
}

/// WiFi network info from scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Network {
    pub ssid: String,
    /// Signal strength in dBm (e.g., -45).
    pub signal: i32,
    /// Security type (e.g., "wpa2", "open").
    pub security: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_start_with_timeout() {
        let json = r#"{"cmd":"start","timeout":60}"#;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(cmd, Command::Start { timeout: 60 });
    }

    #[test]
    fn parse_start_default_timeout() {
        let json = r#"{"cmd":"start"}"#;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(cmd, Command::Start { timeout: 300 });
    }

    #[test]
    fn parse_stop() {
        let json = r#"{"cmd":"stop"}"#;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(cmd, Command::Stop);
    }

    #[test]
    fn parse_status() {
        let json = r#"{"cmd":"status"}"#;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(cmd, Command::Status);
    }

    #[test]
    fn parse_scan() {
        let json = r#"{"cmd":"scan"}"#;
        let cmd: Command = serde_json::from_str(json).unwrap();
        assert_eq!(cmd, Command::Scan);
    }

    #[test]
    fn parse_invalid_command() {
        let json = r#"{"cmd":"invalid"}"#;
        let result: Result<Command, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_cmd_field() {
        let json = r#"{"timeout":60}"#;
        let result: Result<Command, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn parse_malformed_json() {
        let json = r#"{"cmd":"start""#;
        let result: Result<Command, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn serialize_ok_response_minimal() {
        let resp = Response::Ok(OkResponse::new(State::Idle));
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"ok":true,"state":"idle"}"#);
    }

    #[test]
    fn serialize_ok_response_with_remaining() {
        let resp = Response::Ok(OkResponse::new(State::Advertising).with_remaining(245));
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"ok":true,"state":"advertising","remaining":245}"#);
    }

    #[test]
    fn serialize_ok_response_with_networks() {
        let networks = vec![
            Network {
                ssid: "MyWiFi".into(),
                signal: -45,
                security: "wpa2".into(),
            },
            Network {
                ssid: "Guest".into(),
                signal: -72,
                security: "open".into(),
            },
        ];
        let resp = Response::Ok(OkResponse::new(State::Idle).with_networks(networks));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""ssid":"MyWiFi""#));
        assert!(json.contains(r#""signal":-45"#));
    }

    #[test]
    fn serialize_error_response() {
        let resp = Response::Error(ErrorResponse::new("BLE not available"));
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"ok":false,"error":"BLE not available"}"#);
    }

    #[test]
    fn state_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&State::Idle).unwrap(),
            r#""idle""#
        );
        assert_eq!(
            serde_json::to_string(&State::Advertising).unwrap(),
            r#""advertising""#
        );
        assert_eq!(
            serde_json::to_string(&State::Connected).unwrap(),
            r#""connected""#
        );
        assert_eq!(
            serde_json::to_string(&State::Provisioning).unwrap(),
            r#""provisioning""#
        );
    }
}
