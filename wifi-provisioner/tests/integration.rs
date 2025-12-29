//! Integration tests for wifi-provisioner WebSocket API.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;

// Import from the crate.
use wifi_provisioner::websocket::DaemonState;

/// Helper to start test server on a random port.
async fn start_test_server() -> SocketAddr {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let actual_addr = listener.local_addr().unwrap();

    let state = Arc::new(RwLock::new(DaemonState::default()));

    // Spawn server task.
    tokio::spawn(async move {
        loop {
            if let Ok((stream, _peer_addr)) = listener.accept().await {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                    handle_test_connection(ws, state).await;
                });
            }
        }
    });

    actual_addr
}

/// Simplified connection handler for tests.
async fn handle_test_connection(
    ws: WebSocketStream<TcpStream>,
    state: Arc<RwLock<DaemonState>>,
) {
    use wifi_provisioner::protocol::{Command, ErrorResponse, OkResponse, Response, State, Network};

    let (mut write, mut read) = ws.split();

    while let Some(Ok(msg)) = read.next().await {
        if let Message::Text(text) = msg {
            let response = match serde_json::from_str::<Command>(&text) {
                Ok(cmd) => match cmd {
                    Command::Start { timeout } => {
                        let mut s = state.write().await;
                        s.state = State::Advertising;
                        s.advertising_remaining = Some(timeout);
                        Response::Ok(
                            OkResponse::new(State::Advertising)
                                .with_remaining(timeout)
                                .with_wifi_connected(s.wifi_connected),
                        )
                    }
                    Command::Stop => {
                        let mut s = state.write().await;
                        s.state = State::Idle;
                        s.advertising_remaining = None;
                        Response::Ok(OkResponse::new(State::Idle))
                    }
                    Command::Status => {
                        let s = state.read().await;
                        let mut resp = OkResponse::new(s.state)
                            .with_wifi_connected(s.wifi_connected);
                        if let Some(r) = s.advertising_remaining {
                            resp = resp.with_remaining(r);
                        }
                        Response::Ok(resp)
                    }
                    Command::Scan => {
                        let networks = vec![
                            Network {
                                ssid: "TestNetwork".into(),
                                signal: -50,
                                security: "wpa2".into(),
                            },
                        ];
                        Response::Ok(OkResponse::new(State::Idle).with_networks(networks))
                    }
                },
                Err(e) => Response::Error(ErrorResponse::new(format!("Invalid command: {}", e))),
            };

            let json = serde_json::to_string(&response).unwrap();
            write.send(Message::Text(json.into())).await.unwrap();
        }
    }
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
