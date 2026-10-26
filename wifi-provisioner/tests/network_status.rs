//! Exercise the real nmcli adapter without Bluetooth, NetworkManager, or a NIC.
#![cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};
use wifi_provisioner::wifi::{NmcliWifiManager, WifiManager};

#[tokio::test]
async fn activating_profile_does_not_close_startup_setup() {
    let directory =
        std::env::temp_dir().join(format!("wifi-status-adapter-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let command = directory.join("nmcli");
    fs::write(&command, r#"#!/bin/sh
case "$*" in
  '-t -f NAME,TYPE,DEVICE connection show --active')
    printf 'Home:802-11-wireless:wlan0\n'
    ;;
  '-t -f GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION,IP4.ADDRESS,IP6.ADDRESS device show')
    test "$LC_ALL" = C || exit 1
    printf 'GENERAL.DEVICE:wlan0\nGENERAL.TYPE:wifi\nGENERAL.STATE:70 (connecting (getting IP configuration))\nGENERAL.CONNECTION:Home\n'
    ;;
  *) exit 1 ;;
esac
"#).unwrap();
    fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
    // This integration-test executable has one test; no other process's PATH changes.
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &directory);
    let status = NmcliWifiManager::new().status().await;
    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    fs::remove_dir_all(directory).unwrap();
    let status = status.unwrap();
    assert!(
        !status.connected,
        "DHCP in progress must leave startup setup available"
    );
    assert_eq!(status.ssid, None);
}
