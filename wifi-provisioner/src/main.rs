//! wifi-provisioner - BLE daemon for WiFi provisioning.
//!
//! Implements the Improv WiFi protocol for configuring WiFi credentials
//! via Bluetooth LE from a phone or computer.

mod ble;
mod improv;
mod protocol;
mod websocket;
mod wifi;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use ble::{BleConfig, BleEvent, BleManager};
use protocol::State;
use websocket::{DaemonState, ServerConfig};
use wifi::{NmcliWifiManager, WifiManager};

/// Default advertising timeout in seconds.
const DEFAULT_ADVERTISING_TIMEOUT: u32 = 300;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("wifi_provisioner=info".parse().unwrap()),
        )
        .init();

    info!("wifi-provisioner starting");

    // WiFi manager (shared between WebSocket and BLE).
    let wifi = Arc::new(NmcliWifiManager::new());

    // Check initial WiFi connectivity.
    let wifi_connected = match wifi.status().await {
        Ok(status) => {
            if status.connected {
                info!("WiFi already connected to: {}", status.ssid.unwrap_or_default());
                true
            } else {
                info!("WiFi not connected");
                false
            }
        }
        Err(e) => {
            warn!("Failed to check WiFi status: {}", e);
            false
        }
    };

    // Shared daemon state.
    let state = Arc::new(RwLock::new(DaemonState {
        state: if wifi_connected { State::Idle } else { State::Idle },
        advertising_remaining: None,
        wifi_connected,
    }));

    // BLE event channel.
    let (ble_event_tx, mut ble_event_rx) = mpsc::channel::<BleEvent>(16);

    // Get device name (hostname + last 4 of MAC would be ideal, but hostname for now).
    let device_name = get_device_name().await;

    // BLE configuration.
    let ble_config = BleConfig {
        device_name: device_name.clone(),
        firmware_name: "wifi-provisioner".to_string(),
        firmware_version: env!("CARGO_PKG_VERSION").to_string(),
        hardware_type: "RaspberryPi".to_string(),
        redirect_url: format!("http://{}.local:8081", device_name.to_lowercase()),
    };

    // Create BLE manager.
    let ble_manager = Arc::new(BleManager::new(
        ble_config,
        Arc::clone(&wifi),
        ble_event_tx,
    ));
    let _ble_state = ble_manager.state();

    // Clone refs for the spawned tasks.
    let state_for_ws = Arc::clone(&state);
    let wifi_for_ws = Arc::clone(&wifi);
    let state_for_events = Arc::clone(&state);
    let state_for_timeout = Arc::clone(&state);

    // Spawn WebSocket server.
    let ws_config = ServerConfig::default();
    tokio::spawn(async move {
        if let Err(e) = websocket::run_server(ws_config, state_for_ws, wifi_for_ws).await {
            error!("WebSocket server error: {}", e);
        }
    });

    info!("WebSocket server started on 127.0.0.1:8888");

    // Spawn BLE event handler.
    tokio::spawn(async move {
        while let Some(event) = ble_event_rx.recv().await {
            match event {
                BleEvent::Identify => {
                    info!("Identify requested - would flash LED/display message");
                    // TODO: Send WebSocket message to dirtsim UI.
                }
                BleEvent::ClientConnected => {
                    info!("BLE client connected");
                    let mut s = state_for_events.write().await;
                    s.state = State::Connected;
                }
                BleEvent::ClientDisconnected => {
                    info!("BLE client disconnected");
                    let mut s = state_for_events.write().await;
                    if s.state == State::Connected {
                        s.state = State::Advertising;
                    }
                }
                BleEvent::ProvisioningComplete(url) => {
                    info!("Provisioning complete! Redirect URL: {}", url);
                    let mut s = state_for_events.write().await;
                    s.state = State::Idle;
                    s.wifi_connected = true;
                    s.advertising_remaining = None;
                }
            }
        }
    });

    // Spawn advertising timeout handler.
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;

            let mut s = state_for_timeout.write().await;
            if let Some(remaining) = s.advertising_remaining {
                if remaining > 0 {
                    s.advertising_remaining = Some(remaining - 1);
                } else {
                    // Timeout expired.
                    info!("Advertising timeout expired");
                    s.state = State::Idle;
                    s.advertising_remaining = None;
                    // TODO: Actually stop BLE advertising.
                }
            }
        }
    });

    // Auto-start advertising if WiFi not connected.
    if !wifi_connected {
        info!("WiFi not connected, auto-starting BLE advertising");
        let mut s = state.write().await;
        s.state = State::Advertising;
        s.advertising_remaining = Some(DEFAULT_ADVERTISING_TIMEOUT);
        drop(s);

        // Run BLE server (this will block and advertise).
        ble_manager.run().await?;
    } else {
        info!("WiFi connected, BLE advertising on standby");
        info!("Send {{\"cmd\":\"start\"}} to WebSocket to begin advertising");

        // Just run the BLE server in standby mode.
        // In a full implementation, we'd only start advertising when triggered.
        // For now, run it anyway so the service is available.
        ble_manager.run().await?;
    }

    Ok(())
}

/// Get the device name for BLE advertising.
async fn get_device_name() -> String {
    // Try to read hostname.
    if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
        let name = hostname.trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }

    // Fallback.
    "WifiProvisioner".to_string()
}
