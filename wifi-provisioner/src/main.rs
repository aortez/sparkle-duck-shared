//! Bluetooth WiFi provisioning with a bounded, locally controlled setup window.

use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;
use tracing_subscriber::EnvFilter;
use wifi_provisioner::ble::{BleConfig, BleManager};
use wifi_provisioner::websocket::{self, DaemonState, ServerConfig};
use wifi_provisioner::wifi::{NmcliWifiManager, WifiManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    if let Some(command) = std::env::args().nth(1) {
        use futures_util::{SinkExt, StreamExt};
        let request = match command.as_str() {
            "--start" => serde_json::json!({"cmd":"start", "timeout":300}),
            "--stop" => serde_json::json!({"cmd":"stop"}),
            "--status" => serde_json::json!({"cmd":"status"}),
            _ => return Err("Usage: wifi-provisioner [--start|--stop|--status]".into()),
        };
        let (mut socket, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:8888").await?;
        socket
            .send(tokio_tungstenite::tungstenite::Message::Text(
                request.to_string(),
            ))
            .await?;
        let response = tokio::time::timeout(std::time::Duration::from_secs(10), socket.next())
            .await?
            .ok_or("Provisioner disconnected")??;
        let result: serde_json::Value = serde_json::from_str(response.to_text()?)?;
        println!("{}", result);
        if result["ok"] != true {
            return Err("Provisioner command failed".into());
        }
        return Ok(());
    }
    let wifi = Arc::new(NmcliWifiManager::new());
    let wifi_connected = wifi.status().await?.connected;
    let device_name = std::fs::read_to_string("/etc/hostname")?.trim().to_string();
    let port: u16 = std::env::var("WIFI_PROVISIONER_WEB_PORT")
        .unwrap_or_else(|_| "8081".into())
        .parse()?;
    if port == 0 {
        return Err("Invalid device web port".into());
    }
    let config = BleConfig {
        device_name: device_name.clone(),
        redirect_url: format!("http://{}.local:{}", device_name.to_lowercase(), port),
        ..BleConfig::default()
    };
    let state = Arc::new(RwLock::new(DaemonState {
        wifi_connected,
        ..DaemonState::default()
    }));
    let (control, requests) = mpsc::channel(8);
    let manager = BleManager::new(config, Arc::clone(&wifi));
    info!("Starting provisioner; WiFi connected: {}", wifi_connected);
    // A failed control server or BLE task must restart the daemon, not leave a half-working service.
    tokio::try_join!(
        manager.run(requests, Arc::clone(&state), !wifi_connected),
        websocket::run_server(ServerConfig::default(), state, wifi, control),
    )?;
    Ok(())
}
