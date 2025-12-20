SUMMARY = "Hostname setup from boot partition"
DESCRIPTION = "Sets the system hostname from /boot/hostname.txt at boot time. \
This allows customizing the hostname per-device after flashing but before first boot."
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = " \
    file://hostname-setup \
    file://hostname-setup.service \
"

S = "${WORKDIR}"

inherit systemd

SYSTEMD_SERVICE:${PN} = "hostname-setup.service"
SYSTEMD_AUTO_ENABLE = "enable"

# Default hostname if /boot/hostname.txt doesn't exist.
# Override this in your image recipe: HOSTNAME_DEFAULT = "mydevice"
HOSTNAME_DEFAULT ?= "pi"

do_install() {
    # Install the setup script.
    install -d ${D}${sbindir}
    install -m 0755 ${WORKDIR}/hostname-setup ${D}${sbindir}/hostname-setup

    # Substitute the default hostname.
    sed -i "s|@HOSTNAME_DEFAULT@|${HOSTNAME_DEFAULT}|g" ${D}${sbindir}/hostname-setup

    # Install systemd unit.
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${WORKDIR}/hostname-setup.service ${D}${systemd_system_unitdir}/hostname-setup.service
}

FILES:${PN} = " \
    ${sbindir}/hostname-setup \
    ${systemd_system_unitdir}/hostname-setup.service \
"
