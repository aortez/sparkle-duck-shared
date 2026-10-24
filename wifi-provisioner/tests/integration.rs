//! Integration tests for wifi-provisioner WebSocket API.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

// Import from the crate.
use wifi_provisioner::websocket::DaemonState;

/// Exercise the production WebSocket server with a fake BLE controller.
async fn start_test_server() -> SocketAddr {
    use wifi_provisioner::ble::ControlRequest;
    use wifi_provisioner::protocol::{Network, State};
    use wifi_provisioner::wifi::{WifiManager, WifiResult, WifiStatus};
    struct TestWifi;
    impl WifiManager for TestWifi {
        async fn status(&self) -> WifiResult<WifiStatus> {
            Ok(WifiStatus {
                connected: false,
                ssid: None,
            })
        }
        async fn scan(&self) -> WifiResult<Vec<Network>> {
            Ok(vec![Network {
                ssid: "TestNetwork".into(),
                signal: -50,
                security: "wpa2".into(),
            }])
        }
        async fn connect(&self, _: &str, _: &str) -> WifiResult<()> {
            unreachable!()
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let state = Arc::new(RwLock::new(DaemonState::default()));
    let controller_state = Arc::clone(&state);
    let (control, mut requests) = tokio::sync::mpsc::channel::<ControlRequest>(8);
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let mut state = controller_state.write().await;
            state.state = if request.timeout.is_some() {
                State::Advertising
            } else {
                State::Idle
            };
            state.advertising_remaining = request.timeout;
            let _ = request.reply.send(Ok(()));
        }
    });
    tokio::spawn(wifi_provisioner::websocket::run_listener(
        listener,
        state,
        Arc::new(TestWifi),
        control,
    ));
    address
}

/// Connect to WebSocket server.
async fn connect(addr: SocketAddr) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let url = format!("ws://{}", addr);
    let (ws, _) = connect_async(&url).await.expect("Failed to connect");
    ws
}

/// Send a command and receive response.
async fn send_command(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    cmd: serde_json::Value,
) -> serde_json::Value {
    let msg = Message::Text(cmd.to_string().into());
    ws.send(msg).await.expect("Failed to send");

    let resp = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream ended")
        .expect("WebSocket error");

    match resp {
        Message::Text(text) => serde_json::from_str(&text).expect("Invalid JSON response"),
        _ => panic!("Expected text message"),
    }
}

#[tokio::test]
async fn test_status_command() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    let resp = send_command(&mut ws, json!({"cmd": "status"})).await;

    assert_eq!(resp["ok"], true);
    assert_eq!(resp["state"], "idle");
}

#[tokio::test]
async fn test_start_command() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    let resp = send_command(&mut ws, json!({"cmd": "start", "timeout": 60})).await;

    assert_eq!(resp["ok"], true);
    assert_eq!(resp["state"], "advertising");
    assert_eq!(resp["remaining"], 60);
}

#[tokio::test]
async fn test_start_then_status() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    // Start advertising.
    let resp = send_command(&mut ws, json!({"cmd": "start", "timeout": 120})).await;
    assert_eq!(resp["state"], "advertising");

    // Check status shows advertising.
    let resp = send_command(&mut ws, json!({"cmd": "status"})).await;
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["state"], "advertising");
    assert_eq!(resp["remaining"], 120);
}

#[tokio::test]
async fn test_stop_command() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    // Start, then stop.
    send_command(&mut ws, json!({"cmd": "start", "timeout": 60})).await;
    let resp = send_command(&mut ws, json!({"cmd": "stop"})).await;

    assert_eq!(resp["ok"], true);
    assert_eq!(resp["state"], "idle");
}

#[tokio::test]
async fn test_scan_command() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    let resp = send_command(&mut ws, json!({"cmd": "scan"})).await;

    assert_eq!(resp["ok"], true);
    assert!(resp["networks"].is_array());

    let networks = resp["networks"].as_array().unwrap();
    assert!(!networks.is_empty());
    assert!(networks[0]["ssid"].is_string());
    assert!(networks[0]["signal"].is_i64());
}

#[tokio::test]
async fn test_invalid_command() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    let resp = send_command(&mut ws, json!({"cmd": "bogus"})).await;

    assert_eq!(resp["ok"], false);
    assert!(resp["error"].as_str().unwrap().contains("Invalid command"));
}

#[tokio::test]
async fn test_malformed_json() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    // Send raw malformed text.
    ws.send(Message::Text("not json".into())).await.unwrap();

    let resp = timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("Timeout")
        .expect("Stream ended")
        .expect("Error");

    let resp: serde_json::Value = match resp {
        Message::Text(t) => serde_json::from_str(&t).unwrap(),
        _ => panic!("Expected text"),
    };

    assert_eq!(resp["ok"], false);
}

#[tokio::test]
async fn test_multiple_commands_same_connection() {
    let addr = start_test_server().await;
    let mut ws = connect(addr).await;

    // Send multiple commands on same connection.
    let resp1 = send_command(&mut ws, json!({"cmd": "status"})).await;
    let resp2 = send_command(&mut ws, json!({"cmd": "start", "timeout": 30})).await;
    let resp3 = send_command(&mut ws, json!({"cmd": "status"})).await;
    let resp4 = send_command(&mut ws, json!({"cmd": "stop"})).await;
    let resp5 = send_command(&mut ws, json!({"cmd": "status"})).await;

    assert_eq!(resp1["state"], "idle");
    assert_eq!(resp2["state"], "advertising");
    assert_eq!(resp3["state"], "advertising");
    assert_eq!(resp4["state"], "idle");
    assert_eq!(resp5["state"], "idle");
}

#[tokio::test]
async fn reject_browser_origin_and_unbounded_window() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let address = start_test_server().await;
    let mut request = format!("ws://{}", address).into_client_request().unwrap();
    request
        .headers_mut()
        .insert("Origin", "https://example.com".parse().unwrap());
    assert!(connect_async(request).await.is_err());
    let mut ws = connect(address).await;
    for seconds in [0, 301, u32::MAX] {
        let response = send_command(&mut ws, json!({"cmd":"start", "timeout":seconds})).await;
        assert_eq!(response["ok"], false);
    }
}
