//! WebSocket server for local IPC.
//!
//! Listens on 127.0.0.1:8888 and handles commands from local applications.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::protocol::{Command, ErrorResponse, OkResponse, Response, State};
use crate::wifi::WifiManager;

/// Shared daemon state accessible from WebSocket handlers.
#[derive(Debug)]
pub struct DaemonState {
    pub state: State,
    pub advertising_remaining: Option<u32>,
    pub wifi_connected: bool,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            state: State::Idle,
            advertising_remaining: None,
            wifi_connected: false,
        }
    }
}

/// WebSocket server configuration.
pub struct ServerConfig {
    pub addr: SocketAddr,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:8888".parse().unwrap(),
        }
    }
}

/// Shared context for request handlers.
struct HandlerContext<W: WifiManager> {
    state: Arc<RwLock<DaemonState>>,
    wifi: Arc<W>,
}

/// Run the WebSocket server.
///
/// This function runs indefinitely, accepting connections and handling commands.
pub async fn run_server<W: WifiManager + 'static>(
    config: ServerConfig,
    state: Arc<RwLock<DaemonState>>,
    wifi: Arc<W>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(&config.addr).await?;
    info!("WebSocket server listening on {}", config.addr);

    let ctx = Arc::new(HandlerContext { state, wifi });

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let ctx = Arc::clone(&ctx);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, addr, ctx).await {
                        error!("Connection error from {}: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

/// Handle a single WebSocket connection.
async fn handle_connection<W: WifiManager>(
    stream: TcpStream,
    addr: SocketAddr,
    ctx: Arc<HandlerContext<W>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!("New WebSocket connection from {}", addr);

    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("WebSocket read error from {}: {}", addr, e);
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                debug!("Received from {}: {}", addr, text);
                let response = handle_command(&text, &ctx).await;
                let response_json = serde_json::to_string(&response)?;
                debug!("Sending to {}: {}", addr, response_json);
                write.send(Message::Text(response_json.into())).await?;
            }
            Message::Binary(_) => {
                // Binary messages not supported.
                let resp = Response::Error(ErrorResponse::new("Binary messages not supported"));
                let json = serde_json::to_string(&resp)?;
                write.send(Message::Text(json.into())).await?;
            }
            Message::Ping(data) => {
                write.send(Message::Pong(data)).await?;
            }
            Message::Pong(_) => {
                // Ignore pong.
            }
            Message::Close(_) => {
                info!("Client {} closed connection", addr);
                break;
            }
            Message::Frame(_) => {
                // Raw frames not expected.
            }
        }
    }

    info!("Connection closed: {}", addr);
    Ok(())
}

/// Parse and handle a command, returning the appropriate response.
async fn handle_command<W: WifiManager>(text: &str, ctx: &HandlerContext<W>) -> Response {
    let cmd = match serde_json::from_str::<Command>(text) {
        Ok(cmd) => cmd,
        Err(e) => {
            return Response::Error(ErrorResponse::new(format!("Invalid command: {}", e)));
        }
    };

    match cmd {
        Command::Start { timeout } => handle_start(timeout, ctx).await,
        Command::Stop => handle_stop(ctx).await,
        Command::Status => handle_status(ctx).await,
        Command::Scan => handle_scan(ctx).await,
    }
}

/// Handle the "start" command - begin BLE advertising.
async fn handle_start<W: WifiManager>(timeout: u32, ctx: &HandlerContext<W>) -> Response {
    let mut state = ctx.state.write().await;

    // TODO: Actually start BLE advertising in Phase 3.
    state.state = State::Advertising;
    state.advertising_remaining = Some(timeout);

    info!("Started advertising with timeout {}s", timeout);

    Response::Ok(
        OkResponse::new(State::Advertising)
            .with_remaining(timeout)
            .with_wifi_connected(state.wifi_connected),
    )
}

/// Handle the "stop" command - stop BLE advertising.
async fn handle_stop<W: WifiManager>(ctx: &HandlerContext<W>) -> Response {
    let mut state = ctx.state.write().await;

    // TODO: Actually stop BLE advertising in Phase 3.
    state.state = State::Idle;
    state.advertising_remaining = None;

    info!("Stopped advertising");

    Response::Ok(OkResponse::new(State::Idle).with_wifi_connected(state.wifi_connected))
}

/// Handle the "status" command - return current daemon state.
async fn handle_status<W: WifiManager>(ctx: &HandlerContext<W>) -> Response {
    // Check real WiFi status.
    let wifi_connected = match ctx.wifi.status().await {
        Ok(status) => status.connected,
        Err(e) => {
            warn!("Failed to get WiFi status: {}", e);
            false
        }
    };

    let state = ctx.state.read().await;

    let mut resp = OkResponse::new(state.state).with_wifi_connected(wifi_connected);

    if let Some(remaining) = state.advertising_remaining {
        resp = resp.with_remaining(remaining);
    }

    Response::Ok(resp)
}

/// Handle the "scan" command - scan for WiFi networks.
async fn handle_scan<W: WifiManager>(ctx: &HandlerContext<W>) -> Response {
    info!("Scanning for WiFi networks");

    match ctx.wifi.scan().await {
        Ok(networks) => {
            let state = ctx.state.read().await;
            Response::Ok(OkResponse::new(state.state).with_networks(networks))
        }
        Err(e) => {
            error!("WiFi scan failed: {}", e);
            Response::Error(ErrorResponse::new(format!("Scan failed: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Network;
    use crate::wifi::{MockWifiManager, WifiStatus};

    fn make_ctx(wifi: MockWifiManager) -> HandlerContext<MockWifiManager> {
        HandlerContext {
            state: Arc::new(RwLock::new(DaemonState::default())),
            wifi: Arc::new(wifi),
        }
    }

    #[tokio::test]
    async fn handle_status_returns_idle_by_default() {
        let ctx = make_ctx(MockWifiManager::default());
        let resp = handle_command(r#"{"cmd":"status"}"#, &ctx).await;

        match resp {
            Response::Ok(ok) => {
                assert_eq!(ok.state, State::Idle);
                assert_eq!(ok.wifi_connected, Some(false));
            }
            Response::Error(_) => panic!("Expected Ok response"),
        }
    }

    #[tokio::test]
    async fn handle_status_shows_wifi_connected() {
        let mut wifi = MockWifiManager::default();
        wifi.status = WifiStatus {
            connected: true,
            ssid: Some("TestNetwork".into()),
        };
        let ctx = make_ctx(wifi);

        let resp = handle_command(r#"{"cmd":"status"}"#, &ctx).await;

        match resp {
            Response::Ok(ok) => {
                assert_eq!(ok.wifi_connected, Some(true));
            }
            Response::Error(_) => panic!("Expected Ok response"),
        }
    }

    #[tokio::test]
    async fn handle_start_changes_state_to_advertising() {
        let ctx = make_ctx(MockWifiManager::default());
        let resp = handle_command(r#"{"cmd":"start","timeout":120}"#, &ctx).await;

        match resp {
            Response::Ok(ok) => {
                assert_eq!(ok.state, State::Advertising);
                assert_eq!(ok.remaining, Some(120));
            }
            Response::Error(_) => panic!("Expected Ok response"),
        }

        // Verify state was actually updated.
        let state = ctx.state.read().await;
        assert_eq!(state.state, State::Advertising);
    }

    #[tokio::test]
    async fn handle_stop_changes_state_to_idle() {
        let ctx = make_ctx(MockWifiManager::default());

        // Set initial state to advertising.
        {
            let mut state = ctx.state.write().await;
            state.state = State::Advertising;
            state.advertising_remaining = Some(100);
        }

        let resp = handle_command(r#"{"cmd":"stop"}"#, &ctx).await;

        match resp {
            Response::Ok(ok) => {
                assert_eq!(ok.state, State::Idle);
                assert!(ok.remaining.is_none());
            }
            Response::Error(_) => panic!("Expected Ok response"),
        }
    }

    #[tokio::test]
    async fn handle_scan_returns_networks_from_wifi_manager() {
        let mut wifi = MockWifiManager::default();
        wifi.networks = vec![
            Network {
                ssid: "Network1".into(),
                signal: -45,
                security: "wpa2".into(),
            },
            Network {
                ssid: "Network2".into(),
                signal: -60,
                security: "open".into(),
            },
        ];
        let ctx = make_ctx(wifi);

        let resp = handle_command(r#"{"cmd":"scan"}"#, &ctx).await;

        match resp {
            Response::Ok(ok) => {
                assert!(ok.networks.is_some());
                let networks = ok.networks.unwrap();
                assert_eq!(networks.len(), 2);
                assert_eq!(networks[0].ssid, "Network1");
                assert_eq!(networks[1].ssid, "Network2");
            }
            Response::Error(_) => panic!("Expected Ok response"),
        }
    }

    #[tokio::test]
    async fn handle_invalid_command_returns_error() {
        let ctx = make_ctx(MockWifiManager::default());
        let resp = handle_command(r#"{"cmd":"invalid"}"#, &ctx).await;

        match resp {
            Response::Error(err) => {
                assert!(err.error.contains("Invalid command"));
            }
            Response::Ok(_) => panic!("Expected Error response"),
        }
    }

    #[tokio::test]
    async fn handle_malformed_json_returns_error() {
        let ctx = make_ctx(MockWifiManager::default());
        let resp = handle_command(r#"not json"#, &ctx).await;

        match resp {
            Response::Error(err) => {
                assert!(err.error.contains("Invalid command"));
            }
            Response::Ok(_) => panic!("Expected Error response"),
        }
    }
}
