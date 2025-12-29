//! BLE GATT server for Improv WiFi protocol.
//!
//! Implements the Improv WiFi BLE service using bluer.

use std::sync::Arc;
use std::time::Duration;

use bluer::adv::Advertisement;
use bluer::gatt::local::{
    Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
    CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
};
use bluer::{Adapter, Session};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::improv::{
    build_device_info_response, build_provision_response, build_scan_response, capabilities,
    characteristic, ImprovError, ImprovState, RpcCommand, RpcRequest, SERVICE_UUID,
};
use crate::wifi::WifiManager;

/// BLE manager configuration.
pub struct BleConfig {
    /// Device name for advertising (e.g., "DirtSim-A1B2").
    pub device_name: String,
    /// Firmware name for device info.
    pub firmware_name: String,
    /// Firmware version for device info.
    pub firmware_version: String,
    /// Hardware type for device info.
    pub hardware_type: String,
    /// URL to redirect to after successful provisioning.
    pub redirect_url: String,
}

impl Default for BleConfig {
    fn default() -> Self {
        Self {
            device_name: "WifiProvisioner".to_string(),
            firmware_name: "wifi-provisioner".to_string(),
            firmware_version: env!("CARGO_PKG_VERSION").to_string(),
            hardware_type: "RaspberryPi".to_string(),
            redirect_url: "http://dirtsim.local:8081".to_string(),
        }
    }
}

/// Shared state for BLE operations.
pub struct BleState {
    /// Current Improv state.
    pub improv_state: ImprovState,
    /// Current error state.
    pub error_state: ImprovError,
    /// Latest RPC result to be read by client.
    pub rpc_result: Vec<u8>,
    /// Whether advertising is active.
    pub advertising: bool,
}

impl Default for BleState {
    fn default() -> Self {
        Self {
            // Start authorized (no auth required for now).
            improv_state: ImprovState::Authorized,
            error_state: ImprovError::None,
            rpc_result: Vec::new(),
            advertising: false,
        }
    }
}

/// Events from BLE to main application.
#[derive(Debug)]
pub enum BleEvent {
    /// Client requested identify (blink LED, etc.).
    Identify,
    /// Client connected.
    ClientConnected,
    /// Client disconnected.
    ClientDisconnected,
    /// Provisioning succeeded with this URL.
    ProvisioningComplete(String),
}

/// BLE manager for Improv WiFi.
pub struct BleManager<W: WifiManager> {
    config: BleConfig,
    state: Arc<RwLock<BleState>>,
    wifi: Arc<W>,
    event_tx: mpsc::Sender<BleEvent>,
}

impl<W: WifiManager + 'static> BleManager<W> {
    /// Create a new BLE manager.
    pub fn new(
        config: BleConfig,
        wifi: Arc<W>,
        event_tx: mpsc::Sender<BleEvent>,
    ) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(BleState::default())),
            wifi,
            event_tx,
        }
    }

    /// Get the shared state.
    pub fn state(&self) -> Arc<RwLock<BleState>> {
        Arc::clone(&self.state)
    }

    /// Run the BLE GATT server.
    ///
    /// This will:
    /// 1. Initialize the Bluetooth adapter
    /// 2. Register the Improv WiFi GATT service
    /// 3. Start advertising
    /// 4. Handle incoming connections and commands
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Initializing BLE...");

        // Connect to BlueZ.
        let session = Session::new().await?;
        let adapter = session.default_adapter().await?;
        adapter.set_powered(true).await?;

        info!(
            "Using Bluetooth adapter {} ({})",
            adapter.name(),
            adapter.address().await?
        );

        // Set adapter name for advertising.
        adapter.set_alias(self.config.device_name.clone()).await?;

        // Build and register the GATT application.
        let app = self.build_gatt_application().await;
        let _app_handle = adapter.serve_gatt_application(app).await?;

        info!("GATT application registered");

        // Start advertising.
        self.start_advertising(&adapter).await?;

        // Keep the application running.
        // In a real application, this would be coordinated with the main loop.
        info!("BLE server running, waiting for connections...");

        // Wait forever (the handles keep things alive).
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }

        // Cleanup (unreachable but good practice).
        #[allow(unreachable_code)]
        {
            drop(_app_handle);
            Ok(())
        }
    }

    /// Start BLE advertising.
    pub async fn start_advertising(
        &self,
        adapter: &Adapter,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let adv = Advertisement {
            service_uuids: vec![SERVICE_UUID].into_iter().collect(),
            local_name: Some(self.config.device_name.clone()),
            discoverable: Some(true),
            ..Default::default()
        };

        let _handle = adapter.advertise(adv).await?;
        info!("BLE advertising started as '{}'", self.config.device_name);

        {
            let mut state = self.state.write().await;
            state.advertising = true;
        }

        // Keep the handle alive by leaking it.
        // In a real app, we'd store this handle to stop advertising later.
        std::mem::forget(_handle);

        Ok(())
    }

    /// Build the GATT application with Improv WiFi service.
    async fn build_gatt_application(&self) -> Application {
        let state = Arc::clone(&self.state);
        let config = self.config.clone();
        let wifi = Arc::clone(&self.wifi);
        let event_tx = self.event_tx.clone();

        // Capabilities characteristic - read only.
        let capabilities_read = {
            CharacteristicRead {
                read: true,
                fun: Box::new(move |_req| {
                    Box::pin(async move {
                        // We support identify.
                        Ok(vec![capabilities::IDENTIFY])
                    })
                }),
                ..Default::default()
            }
        };

        // Current State characteristic - read + notify.
        let state_for_read = Arc::clone(&state);
        let current_state_read = CharacteristicRead {
            read: true,
            fun: Box::new(move |_req| {
                let state = Arc::clone(&state_for_read);
                Box::pin(async move {
                    let s = state.read().await;
                    Ok(vec![s.improv_state.into()])
                })
            }),
            ..Default::default()
        };

        // Error State characteristic - read + notify.
        let state_for_error = Arc::clone(&state);
        let error_state_read = CharacteristicRead {
            read: true,
            fun: Box::new(move |_req| {
                let state = Arc::clone(&state_for_error);
                Box::pin(async move {
                    let s = state.read().await;
                    Ok(vec![s.error_state.into()])
                })
            }),
            ..Default::default()
        };

        // RPC Result characteristic - read only.
        let state_for_result = Arc::clone(&state);
        let rpc_result_read = CharacteristicRead {
            read: true,
            fun: Box::new(move |_req| {
                let state = Arc::clone(&state_for_result);
                Box::pin(async move {
                    let s = state.read().await;
                    Ok(s.rpc_result.clone())
                })
            }),
            ..Default::default()
        };

        // RPC Command characteristic - write only.
        let state_for_cmd = Arc::clone(&state);
        let wifi_for_cmd = Arc::clone(&wifi);
        let config_for_cmd = config.clone();
        let event_tx_for_cmd = event_tx.clone();

        let rpc_command_write = CharacteristicWrite {
            write: true,
            method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, _req| {
                let state = Arc::clone(&state_for_cmd);
                let wifi = Arc::clone(&wifi_for_cmd);
                let config = config_for_cmd.clone();
                let event_tx = event_tx_for_cmd.clone();

                Box::pin(async move {
                    handle_rpc_command(&new_value, state, wifi, &config, event_tx).await;
                    Ok(())
                })
            })),
            ..Default::default()
        };

        Application {
            services: vec![Service {
                uuid: SERVICE_UUID,
                primary: true,
                characteristics: vec![
                    // Capabilities (read).
                    Characteristic {
                        uuid: characteristic::CAPABILITIES,
                        read: Some(capabilities_read),
                        ..Default::default()
                    },
                    // Current State (read + notify).
                    Characteristic {
                        uuid: characteristic::CURRENT_STATE,
                        read: Some(current_state_read),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Fun(Box::new(|_| {
                                Box::pin(async {})
                            })),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // Error State (read + notify).
                    Characteristic {
                        uuid: characteristic::ERROR_STATE,
                        read: Some(error_state_read),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Fun(Box::new(|_| {
                                Box::pin(async {})
                            })),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // RPC Command (write).
                    Characteristic {
                        uuid: characteristic::RPC_COMMAND,
                        write: Some(rpc_command_write),
                        ..Default::default()
                    },
                    // RPC Result (read + notify).
                    Characteristic {
                        uuid: characteristic::RPC_RESULT,
                        read: Some(rpc_result_read),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Fun(Box::new(|_| {
                                Box::pin(async {})
                            })),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}

impl BleConfig {
    fn clone(&self) -> Self {
        Self {
            device_name: self.device_name.clone(),
            firmware_name: self.firmware_name.clone(),
            firmware_version: self.firmware_version.clone(),
            hardware_type: self.hardware_type.clone(),
            redirect_url: self.redirect_url.clone(),
        }
    }
}

/// Handle an incoming RPC command.
async fn handle_rpc_command<W: WifiManager>(
    data: &[u8],
    state: Arc<RwLock<BleState>>,
    wifi: Arc<W>,
    config: &BleConfig,
    event_tx: mpsc::Sender<BleEvent>,
) {
    debug!("Received RPC command: {:?}", data);

    // Parse the RPC packet.
    let request = match RpcRequest::parse(data) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to parse RPC command: {}", e);
            let mut s = state.write().await;
            s.error_state = ImprovError::InvalidRpc;
            return;
        }
    };

    info!("Processing RPC command: {:?}", request.command);

    match request.command {
        RpcCommand::Identify => {
            // Send identify event to main app.
            let _ = event_tx.send(BleEvent::Identify).await;
        }

        RpcCommand::GetDeviceInfo => {
            let response = build_device_info_response(
                &config.firmware_name,
                &config.firmware_version,
                &config.hardware_type,
                &config.device_name,
            );

            let mut s = state.write().await;
            s.rpc_result = response;
            s.error_state = ImprovError::None;
        }

        RpcCommand::ScanWifiNetworks => {
            // Update state to show we're busy.
            {
                let mut s = state.write().await;
                s.error_state = ImprovError::None;
            }

            // Perform the scan.
            match wifi.scan().await {
                Ok(networks) => {
                    let network_tuples: Vec<(String, i32, bool)> = networks
                        .iter()
                        .map(|n| (n.ssid.clone(), n.signal, n.security != "open"))
                        .collect();

                    let response = build_scan_response(&network_tuples);

                    let mut s = state.write().await;
                    s.rpc_result = response;
                }
                Err(e) => {
                    error!("WiFi scan failed: {}", e);
                    let mut s = state.write().await;
                    s.error_state = ImprovError::Unknown;
                }
            }
        }

        RpcCommand::SendWifiSettings => {
            // Parse credentials.
            let creds = match request.parse_wifi_credentials() {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to parse WiFi credentials: {}", e);
                    let mut s = state.write().await;
                    s.error_state = ImprovError::InvalidRpc;
                    return;
                }
            };

            info!("Attempting to connect to WiFi: {}", creds.ssid);

            // Update state to provisioning.
            {
                let mut s = state.write().await;
                s.improv_state = ImprovState::Provisioning;
                s.error_state = ImprovError::None;
            }

            // Attempt to connect.
            match wifi.connect(&creds.ssid, &creds.password).await {
                Ok(()) => {
                    info!("Successfully connected to WiFi: {}", creds.ssid);

                    let response = build_provision_response(&config.redirect_url);

                    let mut s = state.write().await;
                    s.improv_state = ImprovState::Provisioned;
                    s.rpc_result = response;

                    let _ = event_tx
                        .send(BleEvent::ProvisioningComplete(config.redirect_url.clone()))
                        .await;
                }
                Err(e) => {
                    error!("Failed to connect to WiFi: {}", e);

                    let mut s = state.write().await;
                    s.improv_state = ImprovState::Authorized;
                    s.error_state = ImprovError::UnableToConnect;
                }
            }
        }

        RpcCommand::Hostname => {
            // Hostname setting not implemented yet.
            warn!("Hostname command not implemented");
            let mut s = state.write().await;
            s.error_state = ImprovError::UnknownCommand;
        }
    }
}
