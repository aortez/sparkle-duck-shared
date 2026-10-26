//! WiFi management via NetworkManager (nmcli).
//!
//! Provides a trait-based abstraction for WiFi operations, with a real
//! implementation using nmcli and a mock for testing.

use std::{fs, net::IpAddr, process::Stdio};
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::protocol::Network;

/// Result type for WiFi operations.
pub type WifiResult<T> = Result<T, WifiError>;

/// Errors from WiFi operations.
#[derive(Debug, Clone)]
pub enum WifiError {
    /// nmcli command failed.
    CommandFailed(String),
    /// Failed to parse nmcli output.
    ParseError(String),
    /// WiFi hardware not available.
    NoWifiDevice,
    /// Connection attempt failed.
    ConnectionFailed(String),
}

impl std::fmt::Display for WifiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WifiError::CommandFailed(msg) => write!(f, "nmcli command failed: {}", msg),
            WifiError::ParseError(msg) => write!(f, "Failed to parse nmcli output: {}", msg),
            WifiError::NoWifiDevice => write!(f, "No WiFi device available"),
            WifiError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
        }
    }
}

impl std::error::Error for WifiError {}

/// WiFi connection status.
#[derive(Debug, Clone, PartialEq)]
pub struct WifiStatus {
    /// Whether connected to a WiFi network.
    pub connected: bool,
    /// SSID of current network (if connected).
    pub ssid: Option<String>,
}

/// Trait for WiFi operations.
///
/// This abstraction allows for testing with a mock implementation.
pub trait WifiManager: Send + Sync {
    /// Check current WiFi connection status.
    fn status(&self) -> impl std::future::Future<Output = WifiResult<WifiStatus>> + Send;

    /// Scan for available WiFi networks.
    fn scan(&self) -> impl std::future::Future<Output = WifiResult<Vec<Network>>> + Send;

    /// Connect to a WiFi network.
    fn connect(
        &self,
        ssid: &str,
        password: &str,
    ) -> impl std::future::Future<Output = WifiResult<()>> + Send;
}

/// Real WiFi manager using nmcli.
pub struct NmcliWifiManager;

impl NmcliWifiManager {
    pub fn new() -> Self {
        Self
    }

    /// Run an nmcli command and return stdout.
    async fn run_nmcli(&self, args: &[&str]) -> WifiResult<String> {
        debug!("Running nmcli operation");

        let output = Command::new("nmcli")
            .kill_on_drop(true)
            .env("LC_ALL", "C")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| WifiError::CommandFailed(format!("Failed to execute nmcli: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("nmcli failed: {}", stderr);
            return Err(WifiError::CommandFailed(stderr.into_owned()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl Default for NmcliWifiManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WifiManager for NmcliWifiManager {
    async fn status(&self) -> WifiResult<WifiStatus> {
        // An active connection profile can still be authenticating or waiting
        // for DHCP. Only a usable WiFi link should close the startup BLE window.
        let output = self
            .run_nmcli(&[
                "-t",
                "-f",
                "GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION,IP4.ADDRESS,IP6.ADDRESS",
                "device",
                "show",
            ])
            .await?;
        parse_device_status(&output, |device| {
            if device.contains('/') || matches!(device, "." | "..") {
                return Err(WifiError::ParseError("Invalid interface name".into()));
            }
            match fs::read_to_string(format!("/sys/class/net/{device}/carrier")) {
                Ok(value) if value.trim() == "1" => Ok(true),
                Ok(value) if value.trim() == "0" => Ok(false),
                Ok(_) => Err(WifiError::ParseError("Invalid carrier status".into())),
                Err(error) => Err(WifiError::CommandFailed(error.to_string())),
            }
        })
    }

    async fn scan(&self) -> WifiResult<Vec<Network>> {
        // Trigger a fresh scan first.
        let _ = self.run_nmcli(&["device", "wifi", "rescan"]).await;

        // Get the list of networks.
        let output = self
            .run_nmcli(&["-t", "-f", "SSID,SIGNAL,SECURITY", "device", "wifi", "list"])
            .await?;

        let networks = parse_scan_output(&output);
        info!("Found {} WiFi networks", networks.len());
        Ok(networks)
    }

    async fn connect(&self, ssid: &str, password: &str) -> WifiResult<()> {
        info!("Connecting to WiFi network: {}", ssid);

        // Try to connect. nmcli will create a connection profile if needed.
        let mut args = vec!["--wait", "45", "device", "wifi", "connect", ssid];
        if !password.is_empty() {
            args.extend(["password", password]);
        }
        let result = self.run_nmcli(&args).await;

        match result {
            Ok(output) => {
                if output.contains("successfully activated") {
                    info!("Successfully connected to {}", ssid);
                    Ok(())
                } else {
                    warn!("Unexpected nmcli output: {}", output);
                    // Still might be OK, check status.
                    Ok(())
                }
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", ssid, e);
                Err(WifiError::ConnectionFailed(format!(
                    "Failed to connect to {}: {}",
                    ssid, e
                )))
            }
        }
    }
}

/// nmcli terse output escapes field separators and literal backslashes.
fn parse_nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            fields.last_mut().unwrap().push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            fields.push(String::new());
        } else {
            fields.last_mut().unwrap().push(ch);
        }
    }
    if escaped {
        fields.last_mut().unwrap().push('\\');
    }
    fields
}

/// Device records are blank-line separated. Values can contain unescaped colons
/// (IPv6) as well as nmcli's escaped separators (connection names).
fn parse_device_status(
    output: &str,
    mut carrier: impl FnMut(&str) -> WifiResult<bool>,
) -> WifiResult<WifiStatus> {
    let mut uncertainty = None;
    for block in output.trim().split("\n\n") {
        let fields: Vec<_> = block
            .lines()
            .filter_map(|line| line.split_once(':'))
            .collect();
        let get = |key| fields.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
        let device =
            get("GENERAL.DEVICE").ok_or_else(|| WifiError::ParseError("Missing device".into()))?;
        let kind = get("GENERAL.TYPE")
            .ok_or_else(|| WifiError::ParseError("Missing device type".into()))?;
        // USB Ethernet must not prevent WiFi provisioning.
        if kind != "wifi" {
            continue;
        }
        let state = get("GENERAL.STATE")
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or_else(|| WifiError::ParseError("Missing device state".into()))?;
        if state != 100 {
            continue;
        }
        let has_address = fields.iter().any(|(key, value)| {
            if !key.starts_with("IP4.ADDRESS[") && !key.starts_with("IP6.ADDRESS[") {
                return false;
            }
            let address = unescape_value(value.split('/').next().unwrap_or(""));
            match address.parse::<IpAddr>() {
                Ok(IpAddr::V4(ip)) => {
                    !ip.is_loopback()
                        && !ip.is_link_local()
                        && !ip.is_unspecified()
                        && !ip.is_multicast()
                }
                Ok(IpAddr::V6(ip)) => {
                    !ip.is_loopback()
                        && !ip.is_unicast_link_local()
                        && !ip.is_unspecified()
                        && !ip.is_multicast()
                }
                Err(_) => {
                    uncertainty = Some(WifiError::ParseError("Invalid device address".into()));
                    false
                }
            }
        });
        if has_address {
            match carrier(device) {
                Ok(true) => {
                    return Ok(WifiStatus {
                        connected: true,
                        ssid: get("GENERAL.CONNECTION")
                            .filter(|value| *value != "--")
                            .map(unescape_value),
                    })
                }
                Ok(false) => {}
                Err(error) => uncertainty = Some(error),
            }
        }
    }
    match uncertainty {
        Some(error) => Err(error),
        None => Ok(WifiStatus {
            connected: false,
            ssid: None,
        }),
    }
}

fn unescape_value(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(chars.next().unwrap_or('\\'));
        } else {
            result.push(ch);
        }
    }
    result
}

/// Parse nmcli wifi list output into Network structs.
///
/// Input format (terse mode): `SSID:SIGNAL:SECURITY`
/// Example: `onionchan:65:WPA1 WPA2`
pub fn parse_scan_output(output: &str) -> Vec<Network> {
    let mut networks = Vec::new();
    let mut seen_ssids = std::collections::HashSet::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        // Format: SSID:SIGNAL:SECURITY, with escaped separators in SSIDs.
        let parts = parse_nmcli_fields(line);
        if parts.len() < 3 {
            debug!("Skipping malformed line: {}", line);
            continue;
        }

        let ssid = parts[0].to_string();

        // Skip empty SSIDs (hidden networks).
        if ssid.is_empty() {
            continue;
        }

        // Skip duplicates (same SSID from multiple APs).
        if seen_ssids.contains(&ssid) {
            continue;
        }

        let signal = match parts[1].parse::<i32>() {
            Ok(s) => {
                // nmcli reports signal as 0-100 percentage.
                // Convert to approximate dBm: -100 + percentage.
                -100 + s
            }
            Err(_) => {
                debug!("Skipping line with invalid signal: {}", line);
                continue;
            }
        };

        // Normalize security string.
        let security = normalize_security(&parts[2]);

        seen_ssids.insert(ssid.clone());
        networks.push(Network {
            ssid,
            signal,
            security,
        });
    }

    // Sort by signal strength (strongest first).
    networks.sort_by(|a, b| b.signal.cmp(&a.signal));

    networks
}

/// Normalize security type to a simpler format.
fn normalize_security(raw: &str) -> String {
    let raw_upper = raw.to_uppercase();

    if raw_upper.contains("WPA3") {
        "wpa3".to_string()
    } else if raw_upper.contains("WPA2") {
        "wpa2".to_string()
    } else if raw_upper.contains("WPA") {
        "wpa".to_string()
    } else if raw_upper.contains("WEP") {
        "wep".to_string()
    } else if raw.is_empty() || raw_upper.contains("OPEN") || raw == "--" {
        "open".to_string()
    } else {
        raw.to_lowercase()
    }
}

/// Mock WiFi manager for testing.
#[cfg(test)]
pub struct MockWifiManager {
    pub status: WifiStatus,
    pub networks: Vec<Network>,
    pub connect_result: Result<(), String>,
}

#[cfg(test)]
impl Default for MockWifiManager {
    fn default() -> Self {
        Self {
            status: WifiStatus {
                connected: false,
                ssid: None,
            },
            networks: vec![],
            connect_result: Ok(()),
        }
    }
}

#[cfg(test)]
impl WifiManager for MockWifiManager {
    async fn status(&self) -> WifiResult<WifiStatus> {
        Ok(self.status.clone())
    }

    async fn scan(&self) -> WifiResult<Vec<Network>> {
        Ok(self.networks.clone())
    }

    async fn connect(&self, ssid: &str, _password: &str) -> WifiResult<()> {
        match &self.connect_result {
            Ok(()) => Ok(()),
            Err(msg) => Err(WifiError::ConnectionFailed(format!(
                "Mock connect to {} failed: {}",
                ssid, msg
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_wifi_with_escaped_name_stays_connected() {
        let output = "GENERAL.DEVICE:wlan0\nGENERAL.TYPE:wifi\nGENERAL.STATE:100 (connected)\nGENERAL.CONNECTION:Studio\\:West\\\\Frame\nIP4.ADDRESS[1]:192.168.1.2/24\n";
        let status = parse_device_status(output, |_| Ok(true)).unwrap();
        assert!(status.connected);
        assert_eq!(status.ssid.as_deref(), Some("Studio:West\\Frame"));
    }

    #[test]
    fn startup_setup_stays_open_until_wifi_has_a_usable_address_and_carrier() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/connectivity.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let status = parse_device_status(case["output"].as_str().unwrap(), |device| match case
                ["carrier"][device]
                .as_bool()
            {
                Some(up) => Ok(up),
                None => Err(WifiError::CommandFailed("carrier unavailable".into())),
            });
            if case["wifi_connected"].is_null() {
                assert!(status.is_err(), "{}", case["name"]);
            } else {
                let connected = status.unwrap().connected;
                assert_eq!(
                    connected,
                    case["wifi_connected"].as_bool().unwrap(),
                    "{}",
                    case["name"]
                );
                // main.rs opens startup setup when !connected; ble.rs closes
                // that window only after a status report changes to connected.
                if case["keep_setup_open"] == true {
                    assert!(!connected, "{}", case["name"]);
                }
            }
        }
    }

    #[test]
    fn scan_preserves_colons_and_backslashes_in_ssids() {
        let networks = parse_scan_output("Studio\\:West\\\\Frame:72:WPA2\n");
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "Studio:West\\Frame");
        assert_eq!(networks[0].signal, -28);
    }

    #[test]
    fn parse_basic_scan_output() {
        let output = "turtleback:72:WPA1\nonionchan:65:WPA1 WPA2\n";
        let networks = parse_scan_output(output);

        assert_eq!(networks.len(), 2);

        // Sorted by signal strength.
        assert_eq!(networks[0].ssid, "turtleback");
        assert_eq!(networks[0].signal, -28); // -100 + 72
        assert_eq!(networks[0].security, "wpa");

        assert_eq!(networks[1].ssid, "onionchan");
        assert_eq!(networks[1].signal, -35); // -100 + 65
        assert_eq!(networks[1].security, "wpa2");
    }

    #[test]
    fn parse_empty_output() {
        let networks = parse_scan_output("");
        assert!(networks.is_empty());
    }

    #[test]
    fn parse_skips_empty_ssids() {
        let output = ":50:WPA2\nvisible:60:WPA2\n";
        let networks = parse_scan_output(output);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "visible");
    }

    #[test]
    fn parse_deduplicates_ssids() {
        // Same SSID from multiple APs.
        let output = "mynet:80:WPA2\nmynet:60:WPA2\nmynet:40:WPA2\n";
        let networks = parse_scan_output(output);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "mynet");
        // Should keep the first one seen.
        assert_eq!(networks[0].signal, -20);
    }

    #[test]
    fn parse_skips_malformed_lines() {
        let output = "good:50:WPA2\nbad line\nalso:bad\ngood2:30:open\n";
        let networks = parse_scan_output(output);

        assert_eq!(networks.len(), 2);
    }

    #[test]
    fn parse_handles_open_networks() {
        let output = "opennet:45:\ncafewifi:50:--\n";
        let networks = parse_scan_output(output);

        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].security, "open");
        assert_eq!(networks[1].security, "open");
    }

    #[test]
    fn normalize_security_types() {
        assert_eq!(normalize_security("WPA3"), "wpa3");
        assert_eq!(normalize_security("WPA2 WPA3"), "wpa3");
        assert_eq!(normalize_security("WPA1 WPA2"), "wpa2");
        assert_eq!(normalize_security("WPA2"), "wpa2");
        assert_eq!(normalize_security("WPA1"), "wpa");
        assert_eq!(normalize_security("WPA"), "wpa");
        assert_eq!(normalize_security("WEP"), "wep");
        assert_eq!(normalize_security(""), "open");
        assert_eq!(normalize_security("--"), "open");
    }

    #[test]
    fn signal_conversion_to_dbm() {
        // nmcli reports 0-100, we convert to dBm.
        let output = "net100:100:WPA2\nnet50:50:WPA2\nnet0:0:WPA2\n";
        let networks = parse_scan_output(output);

        assert_eq!(networks[0].signal, 0); // -100 + 100
        assert_eq!(networks[1].signal, -50); // -100 + 50
        assert_eq!(networks[2].signal, -100); // -100 + 0
    }
}
