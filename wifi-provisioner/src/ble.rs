//! BLE GATT server for Improv WiFi protocol.
//!
//! Implements the Improv WiFi BLE service using bluer.

use std::sync::Arc;
use std::time::Duration;

use crate::protocol::State;
use crate::websocket::DaemonState;
use bluer::adv::Advertisement;
use bluer::adv::AdvertisementHandle;
use bluer::gatt::local::{
    characteristic_control, Application, Characteristic, CharacteristicControl,
    CharacteristicControlEvent, CharacteristicNotify, CharacteristicNotifyMethod,
    CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
};
use bluer::gatt::CharacteristicWriter;
use bluer::{Adapter, Session};
use futures_util::StreamExt;
use tokio::sync::{mpsc, oneshot, watch, RwLock};
use tokio::time::Instant;
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
    /// Active RPC result writer for sending notifications to subscribed clients.
    pub rpc_result_notifier: Option<CharacteristicWriter>,
    deadline: Option<Instant>,
    generation: u64,
    busy: bool,
    cancel: watch::Sender<u64>,
    state_changes: watch::Sender<u8>,
    error_changes: watch::Sender<u8>,
}

impl Default for BleState {
    fn default() -> Self {
        Self {
            // An explicit, bounded setup window authorizes credentials.
            improv_state: ImprovState::AuthorizationRequired,
            error_state: ImprovError::None,
            rpc_result: Vec::new(),
            advertising: false,
            rpc_result_notifier: None,
            deadline: None,
            generation: 0,
            busy: false,
            cancel: watch::channel(0).0,
            state_changes: watch::channel(ImprovState::AuthorizationRequired.into()).0,
            error_changes: watch::channel(ImprovError::None.into()).0,
        }
    }
}

/// Requests from trusted local applications. Replies follow the hardware change.
pub struct ControlRequest {
    pub timeout: Option<u32>,
    pub reply: oneshot::Sender<Result<(), String>>,
}

impl BleState {
    fn status(&mut self, phase: ImprovState, error: ImprovError) {
        self.improv_state = phase;
        self.error_state = error;
        self.state_changes.send_replace(phase.into());
        self.error_changes.send_replace(error.into());
    }

    fn authorized(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() < deadline)
    }

    fn close(&mut self) {
        self.deadline = None;
        self.busy = false;
        self.generation += 1;
        self.cancel.send_replace(self.generation);
        self.status(
            ImprovState::AuthorizationRequired,
            ImprovError::NotAuthorized,
        );
    }
}

fn notify_changes(sender: watch::Sender<u8>) -> CharacteristicNotify {
    CharacteristicNotify {
        notify: true,
        method: CharacteristicNotifyMethod::Fun(Box::new(move |mut notifier| {
            let mut changes = sender.subscribe();
            Box::pin(async move {
                loop {
                    let value = *changes.borrow_and_update();
                    if notifier.notify(vec![value]).await.is_err() {
                        break;
                    }
                    tokio::select! {
                        result = changes.changed() => if result.is_err() { break; },
                        _ = notifier.stopped() => break,
                    }
                }
            })
        })),
        ..Default::default()
    }
}

/// BLE manager for Improv WiFi.
pub struct BleManager<W: WifiManager> {
    config: BleConfig,
    state: Arc<RwLock<BleState>>,
    wifi: Arc<W>,
}

impl<W: WifiManager + 'static> BleManager<W> {
    /// Create a new BLE manager.
    pub fn new(config: BleConfig, wifi: Arc<W>) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(BleState::default())),
            wifi,
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
    pub async fn run(
        &self,
        mut requests: mpsc::Receiver<ControlRequest>,
        daemon: Arc<RwLock<DaemonState>>,
        auto_start: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        let (app, rpc_result_control) = self.build_gatt_application().await;
        let _app_handle = adapter.serve_gatt_application(app).await?;

        info!("GATT application registered");

        let mut advertising = None;
        if auto_start {
            self.open_window(&adapter, &mut advertising, 300).await?;
        }

        // Spawn task to handle RPC result notification subscriptions.
        let state_for_notify = Arc::clone(&self.state);
        tokio::spawn(async move {
            let mut control = rpc_result_control;
            while let Some(event) = control.next().await {
                match event {
                    CharacteristicControlEvent::Notify(writer) => {
                        info!("Client subscribed to RPC result notifications");
                        let mut s = state_for_notify.write().await;
                        s.rpc_result_notifier = Some(writer);
                    }
                    CharacteristicControlEvent::Write(_) => {
                        // RPC Result is read-only, ignore writes.
                    }
                }
            }
        });

        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            let request = tokio::select! {
                request = requests.recv() => match request { Some(r) => Some(r), None => break },
                _ = tick.tick() => None,
            };
            let mut reply = None;
            if let Some(request) = request {
                let result = match request.timeout {
                    Some(timeout) => self.open_window(&adapter, &mut advertising, timeout).await,
                    None => {
                        self.state.write().await.close();
                        Ok(())
                    }
                };
                reply = Some((request.reply, result));
            }
            let mut state = self.state.write().await;
            if state.deadline.is_some() && !state.authorized() {
                state.close();
            }
            if state.deadline.is_none() {
                // Dropping the BlueZ handle unregisters the actual advertisement.
                advertising.take();
                state.advertising = false;
            }
            let mut status = daemon.write().await;
            status.state = if state.busy {
                State::Provisioning
            } else if state.advertising {
                State::Advertising
            } else {
                State::Idle
            };
            status.advertising_remaining = state.deadline.map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs()
                    .saturating_add(1) as u32
            });
            if state.improv_state == ImprovState::Provisioned {
                status.wifi_connected = true;
            }
            drop(status);
            drop(state);
            if let Some((reply, result)) = reply {
                let _ = reply.send(result);
            }
        }
        self.state.write().await.close();
        drop(advertising);
        Ok(())
    }

    async fn open_window(
        &self,
        adapter: &Adapter,
        advertising: &mut Option<AdvertisementHandle>,
        timeout: u32,
    ) -> Result<(), String> {
        if !(1..=300).contains(&timeout) {
            return Err("Setup timeout must be 1–300 seconds".into());
        }
        let mut state = self.state.write().await;
        if state.busy {
            return Err("WiFi connection attempt is already in progress".into());
        }
        if advertising.is_none() {
            let adv = Advertisement {
                service_uuids: vec![SERVICE_UUID].into_iter().collect(),
                local_name: Some(self.config.device_name.clone()),
                discoverable: Some(true),
                ..Default::default()
            };
            *advertising = Some(
                adapter
                    .advertise(adv)
                    .await
                    .map_err(|error| error.to_string())?,
            );
        }
        state.close();
        state.deadline = Some(Instant::now() + Duration::from_secs(timeout.into()));
        state.advertising = true;
        state.rpc_result.clear();
        state.status(ImprovState::Authorized, ImprovError::None);
        info!("BLE setup open for {} seconds", timeout);
        Ok(())
    }

    /// Build the GATT application with Improv WiFi service.
    ///
    /// Returns the application and a control handle for the RPC Result characteristic
    /// (used to receive notification subscription events).
    async fn build_gatt_application(&self) -> (Application, CharacteristicControl) {
        let state = Arc::clone(&self.state);
        let config = self.config.clone();
        let wifi = Arc::clone(&self.wifi);

        // Capabilities characteristic - read only.
        let capabilities_read = {
            CharacteristicRead {
                read: true,
                fun: Box::new(move |_req| {
                    Box::pin(async move {
                        // Device info is supported; Identify has no physical implementation.
                        Ok(vec![capabilities::DEVICE_INFO])
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

        // RPC Result characteristic - read + notify (IO-based for push notifications).
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

        // Create control handle for RPC Result notifications.
        let (rpc_result_control, rpc_result_handle) = characteristic_control();

        // RPC Command characteristic - write only.
        let state_for_cmd = Arc::clone(&state);
        let wifi_for_cmd = Arc::clone(&wifi);
        let config_for_cmd = config.clone();

        let rpc_command_write = CharacteristicWrite {
            write: true,
            method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, _req| {
                let state = Arc::clone(&state_for_cmd);
                let wifi = Arc::clone(&wifi_for_cmd);
                let config = config_for_cmd.clone();

                Box::pin(async move {
                    // Acknowledge the GATT write before NetworkManager's potentially slow work.
                    tokio::spawn(async move {
                        handle_rpc_command(&new_value, state, wifi, &config).await;
                    });
                    Ok(())
                })
            })),
            ..Default::default()
        };

        let current_state_notify = notify_changes(state.read().await.state_changes.clone());
        let error_state_notify = notify_changes(state.read().await.error_changes.clone());
        let app = Application {
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
                        notify: Some(current_state_notify),
                        ..Default::default()
                    },
                    // Error State (read + notify).
                    Characteristic {
                        uuid: characteristic::ERROR_STATE,
                        read: Some(error_state_read),
                        notify: Some(error_state_notify),
                        ..Default::default()
                    },
                    // RPC Command (write).
                    Characteristic {
                        uuid: characteristic::RPC_COMMAND,
                        write: Some(rpc_command_write),
                        ..Default::default()
                    },
                    // RPC Result (read + notify via IO for push notifications).
                    Characteristic {
                        uuid: characteristic::RPC_RESULT,
                        read: Some(rpc_result_read),
                        control_handle: rpc_result_handle,
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Io,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        (app, rpc_result_control)
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

/// Send an RPC result notification to the subscribed client.
async fn send_rpc_notification(state: &Arc<RwLock<BleState>>, response: &[u8]) {
    let mut s = state.write().await;
    if let Some(ref mut writer) = s.rpc_result_notifier {
        debug!("Sending RPC result notification ({} bytes)", response.len());
        if !matches!(
            tokio::time::timeout(Duration::from_secs(2), writer.send(response)).await,
            Ok(Ok(()))
        ) {
            warn!("RPC notification failed; result remains readable");
        }
    }
}

/// Handle an incoming RPC command.
async fn handle_rpc_command<W: WifiManager>(
    data: &[u8],
    state: Arc<RwLock<BleState>>,
    wifi: Arc<W>,
    config: &BleConfig,
) {
    debug!("Received RPC command ({} bytes)", data.len());

    // Parse the RPC packet.
    let request = match RpcRequest::parse(data) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to parse RPC command: {}", e);
            let mut s = state.write().await;
            let phase = s.improv_state;
            s.status(phase, ImprovError::InvalidRpc);
            return;
        }
    };

    info!("Processing RPC command: {:?}", request.command);

    match request.command {
        RpcCommand::Identify => {
            let mut s = state.write().await;
            let phase = s.improv_state;
            s.status(phase, ImprovError::UnknownCommand);
        }

        RpcCommand::GetDeviceInfo => {
            let response = build_device_info_response(
                &config.firmware_name,
                &config.firmware_version,
                &config.hardware_type,
                &config.device_name,
            );

            {
                let mut s = state.write().await;
                s.rpc_result = response.clone();
                let phase = s.improv_state;
                s.status(phase, ImprovError::None);
            }
            send_rpc_notification(&state, &response).await;
        }

        RpcCommand::ScanWifiNetworks => {
            // Update state to show we're busy.
            {
                let mut s = state.write().await;
                let phase = s.improv_state;
                s.status(phase, ImprovError::None);
            }

            // Perform the scan.
            match wifi.scan().await {
                Ok(networks) => {
                    let network_tuples: Vec<(String, i32, bool)> = networks
                        .iter()
                        .map(|n| (n.ssid.clone(), n.signal, n.security != "open"))
                        .collect();

                    let response = build_scan_response(&network_tuples);

                    {
                        let mut s = state.write().await;
                        s.rpc_result = response.clone();
                    }
                    send_rpc_notification(&state, &response).await;
                }
                Err(e) => {
                    error!("WiFi scan failed: {}", e);
                    let mut s = state.write().await;
                    let phase = s.improv_state;
                    s.status(phase, ImprovError::Unknown);
                }
            }
        }

        RpcCommand::SendWifiSettings => {
            let (generation, deadline, mut cancelled) = {
                let mut s = state.write().await;
                if !s.authorized() || s.busy {
                    let phase = s.improv_state;
                    s.status(phase, ImprovError::NotAuthorized);
                    return;
                }
                s.busy = true;
                s.rpc_result.clear();
                s.status(ImprovState::Provisioning, ImprovError::None);
                (s.generation, s.deadline.unwrap(), s.cancel.subscribe())
            };
            let creds = match request.parse_wifi_credentials() {
                Ok(creds)
                    if !creds.ssid.is_empty()
                        && creds.ssid.len() <= 32
                        && creds.password.len() <= 64 =>
                {
                    creds
                }
                _ => {
                    let mut s = state.write().await;
                    if s.generation == generation {
                        s.busy = false;
                        s.status(ImprovState::Authorized, ImprovError::InvalidRpc);
                    }
                    return;
                }
            };
            let result = tokio::select! {
                result = wifi.connect(&creds.ssid, &creds.password) => Some(result),
                _ = tokio::time::sleep_until(deadline) => None,
                _ = cancelled.changed() => None,
            };
            let mut s = state.write().await;
            if s.generation != generation {
                return;
            }
            s.busy = false;
            if !s.authorized() || result.is_none() {
                s.close();
                return;
            }
            match result.unwrap() {
                Ok(()) => {
                    let response = build_provision_response(&config.redirect_url);
                    s.rpc_result = response.clone();
                    s.deadline = None;
                    s.status(ImprovState::Provisioned, ImprovError::None);
                    drop(s);
                    send_rpc_notification(&state, &response).await;
                }
                Err(_) => {
                    // Let the client correct the credentials while this setup window is open.
                    s.status(ImprovState::Authorized, ImprovError::UnableToConnect);
                }
            }
        }

        RpcCommand::Hostname => {
            // Hostname setting not implemented yet.
            warn!("Hostname command not implemented");
            let mut s = state.write().await;
            let phase = s.improv_state;
            s.status(phase, ImprovError::UnknownCommand);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::improv::build_response;
    use crate::protocol::Network;
    use crate::wifi::{MockWifiManager, WifiResult, WifiStatus};

    async fn open_state() -> Arc<RwLock<BleState>> {
        let state = Arc::new(RwLock::new(BleState::default()));
        {
            let mut s = state.write().await;
            s.deadline = Some(Instant::now() + Duration::from_secs(30));
            s.status(ImprovState::Authorized, ImprovError::None);
        }
        state
    }

    fn credentials() -> Vec<u8> {
        build_response(
            RpcCommand::SendWifiSettings,
            &["Test network", "test-only-password"],
        )
    }

    #[tokio::test]
    async fn closed_or_expired_setup_rejects_credentials() {
        for expired in [false, true] {
            let state = Arc::new(RwLock::new(BleState::default()));
            if expired {
                state.write().await.deadline = Some(Instant::now());
            }
            handle_rpc_command(
                &credentials(),
                Arc::clone(&state),
                Arc::new(MockWifiManager::default()),
                &BleConfig::default(),
            )
            .await;
            let s = state.read().await;
            assert_eq!(s.error_state, ImprovError::NotAuthorized);
            assert_ne!(s.improv_state, ImprovState::Provisioned);
        }
    }

    #[tokio::test]
    async fn failure_notifies_and_allows_corrected_credentials() {
        let state = open_state().await;
        let mut errors = state.read().await.error_changes.subscribe();
        let wifi = MockWifiManager {
            connect_result: Err("test failure".into()),
            ..MockWifiManager::default()
        };
        handle_rpc_command(
            &credentials(),
            Arc::clone(&state),
            Arc::new(wifi),
            &BleConfig::default(),
        )
        .await;
        errors.changed().await.unwrap();
        assert_eq!(*errors.borrow(), u8::from(ImprovError::UnableToConnect));
        assert_eq!(state.read().await.improv_state, ImprovState::Authorized);
        handle_rpc_command(
            &credentials(),
            Arc::clone(&state),
            Arc::new(MockWifiManager::default()),
            &BleConfig::default(),
        )
        .await;
        assert_eq!(state.read().await.improv_state, ImprovState::Provisioned);
        assert!(!state.read().await.authorized());
        // A known Bluetooth peer cannot reprovision after success.
        handle_rpc_command(
            &credentials(),
            Arc::clone(&state),
            Arc::new(MockWifiManager::default()),
            &BleConfig::default(),
        )
        .await;
        assert_eq!(state.read().await.error_state, ImprovError::NotAuthorized);
    }

    struct BlockedWifi {
        entered: tokio::sync::Notify,
    }
    impl WifiManager for BlockedWifi {
        async fn status(&self) -> WifiResult<WifiStatus> {
            Ok(WifiStatus {
                connected: false,
                ssid: None,
            })
        }
        async fn scan(&self) -> WifiResult<Vec<Network>> {
            Ok(vec![])
        }
        async fn connect(&self, _: &str, _: &str) -> WifiResult<()> {
            self.entered.notify_one();
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn stop_cancels_inflight_work_and_prevents_late_success() {
        let state = open_state().await;
        let wifi = Arc::new(BlockedWifi {
            entered: tokio::sync::Notify::new(),
        });
        let task_state = Arc::clone(&state);
        let task_wifi = Arc::clone(&wifi);
        let task = tokio::spawn(async move {
            handle_rpc_command(&credentials(), task_state, task_wifi, &BleConfig::default()).await;
        });
        wifi.entered.notified().await;
        state.write().await.close();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            state.read().await.improv_state,
            ImprovState::AuthorizationRequired
        );
        assert!(state.read().await.rpc_result.is_empty());
    }

    #[tokio::test]
    async fn busy_session_rejects_second_connection_attempt() {
        let state = open_state().await;
        state.write().await.busy = true;
        handle_rpc_command(
            &credentials(),
            Arc::clone(&state),
            Arc::new(MockWifiManager::default()),
            &BleConfig::default(),
        )
        .await;
        assert_eq!(state.read().await.error_state, ImprovError::NotAuthorized);
        assert!(state.read().await.busy);
    }
}
