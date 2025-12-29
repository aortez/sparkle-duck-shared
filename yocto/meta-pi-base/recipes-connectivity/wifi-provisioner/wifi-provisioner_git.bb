# WiFi Provisioner - BLE daemon for WiFi provisioning.
# Implements Improv WiFi protocol for configuring WiFi via Bluetooth LE.

SUMMARY = "WiFi Provisioner BLE Daemon"
DESCRIPTION = "BLE daemon implementing the Improv WiFi protocol for \
configuring WiFi credentials via Bluetooth LE from a phone or computer."
HOMEPAGE = "https://www.improv-wifi.com/"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit cargo systemd

# Build from local source (externalsrc) during development.
# For production, replace with git SRC_URI.
inherit externalsrc
EXTERNALSRC = "${LAYERDIR}/../../../wifi-provisioner"

# Cargo configuration.
CARGO_SRC_DIR = ""

# Build dependencies (for cross-compilation).
DEPENDS = " \
    dbus \
"

# Runtime dependencies.
RDEPENDS:${PN} = " \
    bluez5 \
    dbus \
    networkmanager \
"

do_install:append() {
    # Install systemd service.
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${EXTERNALSRC}/systemd/wifi-provisioner.service ${D}${systemd_system_unitdir}/
}

# Systemd integration.
SYSTEMD_SERVICE:${PN} = "wifi-provisioner.service"
SYSTEMD_AUTO_ENABLE = "enable"

# Package contents.
FILES:${PN} += " \
    ${systemd_system_unitdir}/wifi-provisioner.service \
"
