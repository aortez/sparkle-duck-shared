//! wifi-provisioner - BLE daemon for WiFi provisioning.
//!
//! Implements the Improv WiFi protocol for configuring WiFi credentials
//! via Bluetooth LE from a phone or computer.

mod protocol;
mod websocket;
mod wifi;

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

use websocket::{DaemonState, ServerConfig};
use wifi::NmcliWifiManager;

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

    // Shared state.
    let state = Arc::new(RwLock::new(DaemonState::default()));

    // WiFi manager.
    let wifi = Arc::new(NmcliWifiManager::new());

    // TODO: Check WiFi connectivity and auto-start advertising if not connected.
    // TODO: Start BLE GATT server (Phase 3).

    // Run WebSocket server (blocks forever).
    let config = ServerConfig::default();
    websocket::run_server(config, state, wifi).await?;

    Ok(())
}
