# WiFi Provisioner

**Status: WIP - Phase 1-4 complete, ready for Yocto packaging**

A lightweight Bluetooth LE daemon for WiFi provisioning on Raspberry Pi. Implements the [Improv WiFi](https://www.improv-wifi.com/) protocol, allowing users to configure WiFi credentials from their phone without needing physical access to the device.

## Overview

When a Pi boots without WiFi (or on demand), this daemon advertises a BLE service. Users open a web page in Chrome, connect via Web Bluetooth, send their network credentials, and the daemon configures NetworkManager.

## Research Summary

### Problem

Devices need a way to configure WiFi when:
- First boot (no credentials yet)
- Network changed (moved to new location)
- Password updated

### Approaches Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **AP mode + captive portal** | Works on any device (iOS, Android, laptop) | Complex stack (hostapd, dnsmasq, web server), user must switch WiFi networks | Rejected |
| **On-device UI** | No external dependencies | Requires touchscreen (inky-soup has none) | Rejected |
| **Bluetooth (Improv WiFi)** | Simple stack, phone stays connected to its WiFi, existing Android app | No iOS support | **Selected** |

### Why Improv WiFi?

- Open protocol with published spec
- Compatible with Home Assistant (auto-discovers devices)
- Web Bluetooth demo available for testing (Chrome)
- Simple BLE GATT service (5 characteristics)
- Aligns with goal of exploring more Bluetooth functionality

### Protocol Summary

Service UUID: `00467768-6228-2272-4663-277478268000`

| Characteristic | UUID Suffix | Purpose |
|----------------|-------------|---------|
| Capabilities | `8005` | Supported features (bit flags) |
| Current State | `8001` | Authorization/Provisioning/Provisioned |
| Error State | `8002` | Error codes |
| RPC Command | `8003` | Receive commands |
| RPC Result | `8004` | Send responses |

Key RPC commands:
- `0x01` - Send WiFi credentials
- `0x02` - Identify (blink LED)
- `0x03` - Get device info
- `0x04` - Scan WiFi networks

### Technology Choices

**Language: Rust**

| Option | Memory | Notes |
|--------|--------|-------|
| Python | ~25 MB | Easy but heavy for Pi Zero |
| Rust | ~3-5 MB | Fits inky-soup ecosystem, small footprint |
| C/C++ | ~1-2 MB | Smallest, but more development effort |

Rust selected for balance of small footprint and development speed.

**WebSocket Library: tokio-tungstenite**

| Library | Status | Notes |
|---------|--------|-------|
| websocket | Deprecated | Old dependencies, avoid |
| tungstenite | Active | Sync only, barebone |
| tokio-tungstenite | Active | Async, 63k dependents, works with bluer |
| warp | Active | Full web framework, heavier |

tokio-tungstenite selected - lightweight, widely used, compatible with bluer (BLE crate).

**IPC: WebSocket on localhost**

Considered Unix sockets, signals, HTTP. WebSocket selected for consistency with dirtsim's architecture and bidirectional communication if needed later.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  wifi-provisioner daemon                                │
│                                                         │
│  ┌──────────────────┐    ┌───────────────────────────┐ │
│  │ WebSocket server │    │ BLE GATT server           │ │
│  │ 127.0.0.1:8888   │    │ (Improv WiFi protocol)    │ │
│  └────────┬─────────┘    └─────────────┬─────────────┘ │
│           │                            │                │
│           ▼                            ▼                │
│  Local apps can:              Phone app can:           │
│  - Trigger advertising        - Discover device        │
│  - Query status               - Send WiFi credentials  │
│  - Request network scan       - Get device info        │
│                                                         │
│                     ┌──────────────┐                   │
│                     │ NetworkManager│                   │
│                     │ (via nmcli)   │                   │
│                     └──────────────┘                   │
└─────────────────────────────────────────────────────────┘
```

### Behavior

1. **On boot**: Check WiFi connectivity
2. **If not connected**: Automatically start BLE advertising
3. **If connected**: Sit idle, wait for trigger
4. **On trigger** (WebSocket command): Start advertising with timeout
5. **On credentials received**: Configure NetworkManager, stop advertising
6. **On timeout**: Stop advertising, return to idle

### WebSocket Protocol

```
→ {"cmd":"start","timeout":300}
← {"ok":true,"state":"advertising"}

→ {"cmd":"status"}
← {"state":"advertising","remaining":245,"connected":false}

→ {"cmd":"scan"}
← {"networks":[{"ssid":"MyWiFi","signal":-45,"security":"wpa2"}]}

→ {"cmd":"stop"}
← {"ok":true,"state":"idle"}
```

## End User Experience

### First Boot Flow

1. User unboxes device, plugs it in
2. Device boots, has no WiFi configured
3. Daemon starts BLE advertising automatically
4. User scans QR code on device (or follows printed instructions)
5. QR code opens `https://allan.pizza/wifi` in Chrome
6. User taps "Connect", Chrome shows Bluetooth device picker
7. User selects their device (e.g., "DirtSim-A1B2")
8. Page shows available WiFi networks
9. User selects network, enters password
10. Device connects to WiFi, page shows success with link to device UI

### Web Setup Page

Hosted at `https://allan.pizza/wifi` using the [Improv WiFi JavaScript SDK](https://github.com/improv-wifi/sdk-ble-js).

Single page works for all Sparkle Duck devices (dirtsim, inky-soup, etc.) — the Improv protocol handles device-specific info.

**Browser support:**
- Chrome (Android, Windows, macOS, Linux): ✅
- Firefox, Safari, iOS: ❌ (no Web Bluetooth support)

The page will detect unsupported browsers and show a helpful message with alternatives.

## Design Decisions

### BLE Device Naming

Devices advertise using their hostname for identification:
- Format: `<Hostname>-<Last4MAC>` (e.g., `DirtSim-A1B2`)
- Allows distinguishing multiple devices in the Bluetooth picker

### Return URL

After successful WiFi connection, the daemon returns the device's web UI URL:
- dirtsim: `http://<hostname>.local:8081`
- inky-soup: `http://<hostname>.local:8000`

The setup page displays this as a clickable link: "Setup complete! [Open DirtSim →]"

### Identify Command

Improv's Identify command (`0x02`) helps users find their device when multiple are nearby:
- **dirtsim**: Flash a message on LVGL display
- **inky-soup**: Skip (e-ink too slow for visual feedback)

### iOS / Unsupported Browser Handling

Web Bluetooth is Chrome-only. For unsupported browsers, the setup page shows:

```
This page requires Chrome for Bluetooth support.

Alternatives:
• Open this page in Chrome on Android or a computer
• Use Home Assistant (auto-discovers Improv devices)
```

### Hostname Configuration

Setting hostname during provisioning (Improv command `0x05`) is a Phase 2 feature. Initial implementation uses the hostname set at flash time via `/boot/hostname.txt`.

### Future: AP Mode Fallback

For broader device support (iOS, non-Chrome browsers), a future enhancement could add AP mode as a fallback triggered by a physical button. Not in initial scope.

## Implementation Plan

### Phase 1: Project Skeleton ✅
- [x] Initialize Cargo project with dependencies
- [x] Basic tokio async main
- [x] WebSocket server accepting connections
- [x] Command/response types with serde
- [x] Simple command handling (stub responses)
- [x] **Unit tests:** Command parsing (all variants, invalid input)
- [x] **Unit tests:** Response serialization (success, error, optional fields)
- [x] **Integration test:** Connect to WebSocket, send command, verify response

### Phase 2: WiFi Operations ✅
- [x] WifiManager trait (abstraction for testing)
- [x] NmcliWifiManager implementation
- [x] Query connection status via nmcli
- [x] Scan available networks via nmcli
- [x] Connect to network via nmcli
- [x] Parse nmcli output formats
- [x] **Unit tests:** nmcli output parsing (various formats, edge cases)
- [x] **Unit tests:** Mock WifiManager for command handler logic
- [x] **Integration test:** WebSocket scan command returns real networks (on Pi)
- [x] **Manual test:** Verify on real Pi with real WiFi

### Phase 3: BLE GATT Server ✅
- [x] Improv protocol constants (UUIDs, command IDs, states)
- [x] RPC packet parsing (checksum validation, payload extraction)
- [x] RPC response building (device info, scan results, errors)
- [x] Initialize bluer and BlueZ connection
- [x] Register Improv WiFi service and characteristics
- [x] Implement advertising start/stop
- [x] Handle RPC command characteristic writes
- [x] Send responses via RPC result characteristic
- [x] **Unit tests:** Checksum calculation
- [x] **Unit tests:** Parse WiFi credentials packet
- [x] **Unit tests:** Build device info response
- [x] **Unit tests:** Parse/reject malformed packets
- [ ] **Integration test:** BLE advertises (check with `bluetoothctl`) — requires Pi

### Phase 4: Integration ✅
- [x] Combine WebSocket + BLE in single async runtime
- [x] Shared state between WebSocket commands and BLE events
- [x] Auto-advertise on boot if no WiFi
- [x] Timeout handling (stop advertising after N seconds)
- [x] Return URL after successful connection
- [ ] **Integration test:** WebSocket start → BLE advertises — requires Pi
- [ ] **Integration test:** Timeout expires → state returns to idle
- [ ] **End-to-end test:** Web Bluetooth demo connects and provisions (manual)

### Phase 5: Yocto Packaging
- [ ] Create recipe for wifi-provisioner
- [ ] systemd service file
- [ ] Add to pi-base-image.bbclass
- [ ] **Integration test:** Service starts on boot
- [ ] **Integration test:** Service auto-advertises when WiFi disconnected
- [ ] Test in dirtsim and inky-soup images

### Phase 6: Web Setup Page
- [ ] Create wifi.html page using Improv WiFi JS SDK
- [ ] Add to aortez.github.io (allan.pizza/wifi)
- [ ] Browser detection with helpful fallback message
- [ ] **End-to-end test:** Full flow from QR scan to connected device
- [ ] Create QR code for printed instructions

## Project Structure

```
wifi-provisioner/
├── README.md
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs           # Entry point, runs WebSocket + BLE servers
│   ├── lib.rs            # Library exports for testing
│   ├── protocol.rs       # WebSocket command/response types
│   ├── websocket.rs      # WebSocket server + command handling
│   ├── wifi.rs           # WifiManager trait + NmcliWifiManager
│   ├── ble.rs            # BLE GATT server using bluer
│   └── improv.rs         # Improv protocol constants + RPC parsing
├── tests/
│   └── integration.rs    # WebSocket integration tests
└── systemd/
    └── wifi-provisioner.service  # systemd unit (Phase 5)
```

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "sync", "macros", "time", "process"] }
tokio-tungstenite = "0.24"
futures-util = "0.3"
bluer = { version = "0.17", features = ["bluetoothd"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tokio-test = "0.4"
```

## Building

TODO

## Testing

### Unit Tests

```bash
cargo test
```

Unit tests cover pure logic that doesn't require external systems:
- Command JSON parsing
- Response JSON serialization
- Improv protocol byte parsing/building
- nmcli output parsing
- Checksum calculations

### Integration Tests

```bash
# Run integration tests (requires the daemon to NOT be running)
cargo test --test integration

# Or run a specific integration test
cargo test --test integration websocket_round_trip
```

Integration tests start a real WebSocket server and verify end-to-end behavior.

### Manual Testing

**WebSocket (with websocat or similar):**
```bash
# Start the daemon
cargo run

# In another terminal, connect and send commands
websocat ws://127.0.0.1:8888
{"cmd":"status"}
{"cmd":"scan"}
{"cmd":"start","timeout":60}
```

**BLE (with bluetoothctl):**
```bash
# Check if advertising
bluetoothctl
> scan on
# Look for device name like "DirtSim-XXXX"

# Or check adapter status
bluetoothctl show
```

**End-to-end (with Web Bluetooth):**
1. Open https://jnthas.github.io/improv-wifi-demo/ in Chrome
2. Click Connect
3. Select device from Bluetooth picker
4. Verify device info appears
5. Send WiFi credentials
6. Verify device connects

## Compatible Clients

By implementing the standard Improv WiFi protocol, this daemon works with:

| Client | Platform | Notes |
|--------|----------|-------|
| [allan.pizza/wifi](https://allan.pizza/wifi) | Chrome (Android/Desktop) | Our setup page (Phase 6) |
| [Web Bluetooth Demo](https://jnthas.github.io/improv-wifi-demo/) | Chrome (Android/Desktop) | Third-party, good for testing |
| [Home Assistant](https://www.home-assistant.io/integrations/improv_ble) | Any (with Bluetooth) | Auto-discovers devices |
| [Android SDK Demo](https://github.com/improv-wifi/sdk-android) | Android | Build from source |

**Note:** There is no standalone Improv WiFi app on the Play Store. The Android SDK is meant for integration into other apps.

## References

- [Improv WiFi Specification](https://www.improv-wifi.com/)
- [Improv WiFi BLE Details](https://www.improv-wifi.com/ble/)
- [Improv WiFi SDKs](https://www.improv-wifi.com/code/)
- [Home Assistant Improv BLE Integration](https://www.home-assistant.io/integrations/improv_ble)
- [bluer crate documentation](https://docs.rs/bluer)
- [tokio-tungstenite examples](https://github.com/snapview/tokio-tungstenite)
