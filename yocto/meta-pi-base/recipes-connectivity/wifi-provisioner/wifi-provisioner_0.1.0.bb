# WiFi Provisioner - BLE daemon for WiFi provisioning.
# Implements Improv WiFi protocol for configuring WiFi via Bluetooth LE.
#
# Recipe generated with cargo-bitbake and customized for Yocto integration.

inherit cargo systemd

SUMMARY = "WiFi Provisioner BLE Daemon"
DESCRIPTION = "BLE daemon implementing the Improv WiFi protocol for \
configuring WiFi credentials via Bluetooth LE from a phone or computer."
HOMEPAGE = "https://github.com/aortez/sparkle-duck-shared"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

# Source: git repository + crate dependencies.
SRC_URI = " \
    git://github.com/aortez/sparkle-duck-shared.git;protocol=https;branch=main \
    crate://crates.io/aho-corasick/1.1.4 \
    crate://crates.io/async-stream-impl/0.3.6 \
    crate://crates.io/async-stream/0.3.6 \
    crate://crates.io/autocfg/1.5.0 \
    crate://crates.io/bitflags/2.10.0 \
    crate://crates.io/block-buffer/0.10.4 \
    crate://crates.io/bluer/0.17.4 \
    crate://crates.io/bumpalo/3.19.1 \
    crate://crates.io/byteorder/1.5.0 \
    crate://crates.io/bytes/1.11.0 \
    crate://crates.io/cfg-if/1.0.4 \
    crate://crates.io/cfg_aliases/0.2.1 \
    crate://crates.io/cpufeatures/0.2.17 \
    crate://crates.io/crypto-common/0.1.7 \
    crate://crates.io/custom_debug/0.6.2 \
    crate://crates.io/custom_debug_derive/0.6.2 \
    crate://crates.io/darling/0.20.11 \
    crate://crates.io/darling_core/0.20.11 \
    crate://crates.io/darling_macro/0.20.11 \
    crate://crates.io/data-encoding/2.9.0 \
    crate://crates.io/dbus-crossroads/0.5.3 \
    crate://crates.io/dbus-tokio/0.7.6 \
    crate://crates.io/dbus/0.9.10 \
    crate://crates.io/digest/0.10.7 \
    crate://crates.io/displaydoc/0.2.5 \
    crate://crates.io/errno/0.3.14 \
    crate://crates.io/fnv/1.0.7 \
    crate://crates.io/futures-channel/0.3.31 \
    crate://crates.io/futures-core/0.3.31 \
    crate://crates.io/futures-executor/0.3.31 \
    crate://crates.io/futures-io/0.3.31 \
    crate://crates.io/futures-macro/0.3.31 \
    crate://crates.io/futures-sink/0.3.31 \
    crate://crates.io/futures-task/0.3.31 \
    crate://crates.io/futures-util/0.3.31 \
    crate://crates.io/futures/0.3.31 \
    crate://crates.io/generic-array/0.14.7 \
    crate://crates.io/getrandom/0.2.16 \
    crate://crates.io/getrandom/0.3.4 \
    crate://crates.io/heck/0.5.0 \
    crate://crates.io/hex/0.4.3 \
    crate://crates.io/http/1.4.0 \
    crate://crates.io/httparse/1.10.1 \
    crate://crates.io/ident_case/1.0.1 \
    crate://crates.io/itoa/1.0.17 \
    crate://crates.io/js-sys/0.3.83 \
    crate://crates.io/lazy_static/1.5.0 \
    crate://crates.io/libc/0.2.178 \
    crate://crates.io/libdbus-sys/0.2.7 \
    crate://crates.io/log/0.4.29 \
    crate://crates.io/macaddr/1.0.1 \
    crate://crates.io/matchers/0.2.0 \
    crate://crates.io/memchr/2.7.6 \
    crate://crates.io/mio/1.1.1 \
    crate://crates.io/nix/0.29.0 \
    crate://crates.io/nu-ansi-term/0.50.3 \
    crate://crates.io/num-derive/0.4.2 \
    crate://crates.io/num-traits/0.2.19 \
    crate://crates.io/once_cell/1.21.3 \
    crate://crates.io/pin-project-internal/1.1.10 \
    crate://crates.io/pin-project-lite/0.2.16 \
    crate://crates.io/pin-project/1.1.10 \
    crate://crates.io/pin-utils/0.1.0 \
    crate://crates.io/pkg-config/0.3.32 \
    crate://crates.io/ppv-lite86/0.2.21 \
    crate://crates.io/proc-macro2/1.0.104 \
    crate://crates.io/quote/1.0.42 \
    crate://crates.io/r-efi/5.3.0 \
    crate://crates.io/rand/0.8.5 \
    crate://crates.io/rand_chacha/0.3.1 \
    crate://crates.io/rand_core/0.6.4 \
    crate://crates.io/regex-automata/0.4.13 \
    crate://crates.io/regex-syntax/0.8.8 \
    crate://crates.io/rustversion/1.0.22 \
    crate://crates.io/serde/1.0.228 \
    crate://crates.io/serde_core/1.0.228 \
    crate://crates.io/serde_derive/1.0.228 \
    crate://crates.io/serde_json/1.0.148 \
    crate://crates.io/sha1/0.10.6 \
    crate://crates.io/sharded-slab/0.1.7 \
    crate://crates.io/signal-hook-registry/1.4.8 \
    crate://crates.io/slab/0.4.11 \
    crate://crates.io/smallvec/1.15.1 \
    crate://crates.io/socket2/0.6.1 \
    crate://crates.io/strsim/0.11.1 \
    crate://crates.io/strum/0.26.3 \
    crate://crates.io/strum_macros/0.26.4 \
    crate://crates.io/syn/2.0.111 \
    crate://crates.io/synstructure/0.13.2 \
    crate://crates.io/thiserror-impl/1.0.69 \
    crate://crates.io/thiserror/1.0.69 \
    crate://crates.io/thread_local/1.1.9 \
    crate://crates.io/tokio-macros/2.6.0 \
    crate://crates.io/tokio-stream/0.1.17 \
    crate://crates.io/tokio-test/0.4.4 \
    crate://crates.io/tokio-tungstenite/0.24.0 \
    crate://crates.io/tokio/1.48.0 \
    crate://crates.io/tracing-attributes/0.1.31 \
    crate://crates.io/tracing-core/0.1.36 \
    crate://crates.io/tracing-log/0.2.0 \
    crate://crates.io/tracing-subscriber/0.3.22 \
    crate://crates.io/tracing/0.1.44 \
    crate://crates.io/tungstenite/0.24.0 \
    crate://crates.io/typenum/1.19.0 \
    crate://crates.io/unicode-ident/1.0.22 \
    crate://crates.io/utf-8/0.7.6 \
    crate://crates.io/uuid/1.19.0 \
    crate://crates.io/valuable/0.1.1 \
    crate://crates.io/version_check/0.9.5 \
    crate://crates.io/wasi/0.11.1+wasi-snapshot-preview1 \
    crate://crates.io/wasip2/1.0.1+wasi-0.2.4 \
    crate://crates.io/wasm-bindgen-macro-support/0.2.106 \
    crate://crates.io/wasm-bindgen-macro/0.2.106 \
    crate://crates.io/wasm-bindgen-shared/0.2.106 \
    crate://crates.io/wasm-bindgen/0.2.106 \
    crate://crates.io/windows-link/0.2.1 \
    crate://crates.io/windows-sys/0.59.0 \
    crate://crates.io/windows-sys/0.60.2 \
    crate://crates.io/windows-sys/0.61.2 \
    crate://crates.io/windows-targets/0.52.6 \
    crate://crates.io/windows-targets/0.53.5 \
    crate://crates.io/windows_aarch64_gnullvm/0.52.6 \
    crate://crates.io/windows_aarch64_gnullvm/0.53.1 \
    crate://crates.io/windows_aarch64_msvc/0.52.6 \
    crate://crates.io/windows_aarch64_msvc/0.53.1 \
    crate://crates.io/windows_i686_gnu/0.52.6 \
    crate://crates.io/windows_i686_gnu/0.53.1 \
    crate://crates.io/windows_i686_gnullvm/0.52.6 \
    crate://crates.io/windows_i686_gnullvm/0.53.1 \
    crate://crates.io/windows_i686_msvc/0.52.6 \
    crate://crates.io/windows_i686_msvc/0.53.1 \
    crate://crates.io/windows_x86_64_gnu/0.52.6 \
    crate://crates.io/windows_x86_64_gnu/0.53.1 \
    crate://crates.io/windows_x86_64_gnullvm/0.52.6 \
    crate://crates.io/windows_x86_64_gnullvm/0.53.1 \
    crate://crates.io/windows_x86_64_msvc/0.52.6 \
    crate://crates.io/windows_x86_64_msvc/0.53.1 \
    crate://crates.io/wit-bindgen/0.46.0 \
    crate://crates.io/zerocopy-derive/0.8.31 \
    crate://crates.io/zerocopy/0.8.31 \
    crate://crates.io/zmij/1.0.2 \
"

# Pin to the commit with BLE notification and boot fixes.
SRCREV = "c47c953fcb41a4b8945bc315cb1704a9a97004ed"
S = "${WORKDIR}/git/wifi-provisioner"

# Downgrade Cargo.lock from version 4 to version 3 for Rust 1.75 compatibility.
do_configure:prepend() {
    sed -i 's/^version = 4$/version = 3/' ${S}/Cargo.lock
}

# Build dependencies.
DEPENDS = "dbus pkgconfig-native"

# Runtime dependencies.
RDEPENDS:${PN} = " \
    bluez5 \
    dbus \
    networkmanager \
"

# Install systemd service.
do_install:append() {
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${S}/systemd/wifi-provisioner.service ${D}${systemd_system_unitdir}/
}

# Systemd integration.
SYSTEMD_SERVICE:${PN} = "wifi-provisioner.service"
SYSTEMD_AUTO_ENABLE = "enable"

# Package contents.
FILES:${PN} += "${systemd_system_unitdir}/wifi-provisioner.service"

# Include crate checksums.
require wifi-provisioner-crates.inc
