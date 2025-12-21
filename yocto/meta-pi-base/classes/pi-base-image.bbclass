# pi-base-image.bbclass
# Base image class for Raspberry Pi projects in the Sparkle Duck family.
# Provides A/B boot, persistent data, NetworkManager, and SSH access.
#
# Usage in your image recipe:
#   inherit pi-base-image
#   BOOT_DEVICE = "mmcblk0"   # Required: "mmcblk0" for SD, "sda" for USB
#   IMAGE_INSTALL:append = " my-app"
#
# Optional variables:
#   HOSTNAME_DEFAULT = "mydevice"   # Default hostname (default: "pi")
#   BOOT_SIZE = "150"               # Boot partition size in MB
#   ROOTFS_SIZE = "800"             # Root filesystem size in MB
#   DATA_SIZE = "100"               # Data partition size in MB

inherit core-image

# Require BOOT_DEVICE to be set.
BOOT_DEVICE ?= ""
python () {
    boot_device = d.getVar('BOOT_DEVICE')
    if not boot_device:
        bb.fatal("BOOT_DEVICE must be set in your image recipe. Use 'mmcblk0' for SD card or 'sda' for USB boot.")
}

# Use A/B partition layout.
WKS_FILE = "sdimage-ab.wks.in"
WKS_SEARCH_PATH:prepend = "${LAYERDIR_pi-base}/wic:"

# Enable systemd.
INIT_MANAGER = "systemd"
DISTRO_FEATURES:append = " systemd"
DISTRO_FEATURES_BACKFILL_CONSIDERED:append = " sysvinit"
VIRTUAL-RUNTIME_init_manager = "systemd"
VIRTUAL-RUNTIME_initscripts = "systemd-compat-units"

# SSH access.
IMAGE_FEATURES += "ssh-server-openssh"

# A/B boot management.
IMAGE_INSTALL:append = " \
    ab-boot-manager \
"

# Persistent data partition (WiFi credentials survive updates).
IMAGE_INSTALL:append = " \
    persistent-data \
"

# Hostname setup from /boot/hostname.txt.
IMAGE_INSTALL:append = " \
    hostname-setup \
"

# Network management.
IMAGE_INSTALL:append = " \
    networkmanager \
    networkmanager-nmtui \
    networkmanager-nmcli \
"

# Time synchronization.
IMAGE_INSTALL:append = " \
    systemd-timesyncd \
"

# Service discovery (mDNS).
IMAGE_INSTALL:append = " \
    avahi-daemon \
    avahi-utils \
"

# Useful base utilities.
IMAGE_INSTALL:append = " \
    sudo \
    curl \
    less \
    nano \
    htop \
"

# Mark initial boot slot.
setup_ab_boot() {
    install -d ${IMAGE_ROOTFS}/boot
    echo "a" > ${IMAGE_ROOTFS}/boot/boot_slot
}
ROOTFS_POSTPROCESS_COMMAND:append = " setup_ab_boot;"
