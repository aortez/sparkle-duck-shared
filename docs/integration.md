# Integration Guide

Step-by-step guide for integrating sparkle-duck-shared into your Raspberry Pi project.

## 1. Add as Git Submodule

```bash
cd your-project/yocto/src/   # or yocto/sources/
git submodule add https://github.com/aortez/sparkle-duck-shared.git pi-base
git commit -m "Add pi-base shared infrastructure"
```

## 2. Update bblayers.conf

Add meta-pi-base to your `conf/bblayers.conf`:

```bitbake
BBLAYERS ?= " \
  ${TOPDIR}/../poky/meta \
  ${TOPDIR}/../poky/meta-poky \
  ${TOPDIR}/../meta-openembedded/meta-oe \
  ${TOPDIR}/../meta-openembedded/meta-networking \
  ${TOPDIR}/../meta-raspberrypi \
  ${TOPDIR}/../pi-base/yocto/meta-pi-base \
  ${TOPDIR}/../meta-yourproject \
"
```

## 3. Configure Your Image Recipe

### Option A: Inherit pi-base-image (Recommended)

Create or update your image recipe to inherit from `pi-base-image`:

```bitbake
# recipes-core/images/myproject-image.bb
SUMMARY = "My Project Image"
LICENSE = "MIT"

inherit pi-base-image

# Required: Set boot device for your Pi model.
# "mmcblk0" for SD card (Pi Zero 2W, Pi 3, Pi 4)
# "sda" for USB boot (Pi 4, Pi 5)
BOOT_DEVICE = "mmcblk0"

# Required: Configure the app-owned persistent directory under /data.
# Set these via local.conf / KAS so they're available when building persistent-data.
# PERSISTENT_DATA_APP_DIR = "myproject"
# PERSISTENT_DATA_APP_USER = "myuser"
# PERSISTENT_DATA_APP_GROUP = "mygroup"  # Optional (defaults to user)

# Optional: Override defaults.
HOSTNAME_DEFAULT = "myproject"
# BOOT_SIZE = "150"
# ROOTFS_SIZE = "800"
# DATA_SIZE = "100"

# Add your project-specific packages.
IMAGE_INSTALL:append = " \
    myproject-server \
    myproject-ui \
"
```

### Option B: Manual Configuration

If you need more control, add recipes individually:

```bitbake
inherit core-image

IMAGE_INSTALL:append = " \
    ab-boot-manager \
    persistent-data \
    hostname-setup \
    networkmanager \
    avahi-daemon \
"

WKS_FILE = "sdimage-ab.wks.in"
WKS_SEARCH_PATH:prepend = "${LAYERDIR_pi-base}/wic:"

BOOT_DEVICE = "mmcblk0"
```

## 4. Update Your Flash Script

Refactor your `flash.mjs` to use shared utilities:

```javascript
#!/usr/bin/env node
import { dirname, join, basename } from 'path';
import { fileURLToPath } from 'url';
import { existsSync } from 'fs';

// Import shared utilities.
import {
  colors, log, info, success, warn, error, prompt, formatBytes,
  loadConfig, saveConfig,
  findSshKeys, configureSSHKey, injectSSHKey,
  hasDataPartition, backupDataPartition, restoreDataPartition, cleanupBackup, setHostname,
  getBlockDevices, findLatestImage, flashImage, selectDevice,
  getWifiCredentials, injectWifiCredentials,
} from '../pi-base/scripts/lib/index.mjs';

// Project-specific configuration.
const __dirname = dirname(fileURLToPath(import.meta.url));
const YOCTO_DIR = dirname(__dirname);
const IMAGE_DIR = join(YOCTO_DIR, 'build/tmp/deploy/images/raspberrypi0-2w');
const CONFIG_FILE = join(YOCTO_DIR, '.flash-config.json');
const WIFI_CREDS_FILE = join(YOCTO_DIR, 'wifi-creds.local');
const DEFAULT_HOSTNAME = 'myproject';
const USERNAME = 'root';  // or your custom user
const USER_UID = 0;       // 0 for root, 1000 for first user

async function main() {
  log('');
  log(`${colors.bold}${colors.cyan}My Project Flash Tool${colors.reset}`);
  log('');

  // Load or configure SSH key.
  let config = loadConfig(CONFIG_FILE);
  if (!config) {
    config = await configureSSHKey(CONFIG_FILE);
  } else {
    info(`Using SSH key: ${basename(config.ssh_key_path)}`);
  }

  // Find image.
  const image = findLatestImage(IMAGE_DIR, '.wic.gz', [
    'myproject-image-raspberrypi0-2w.rootfs.wic.gz',
  ]);
  if (!image) {
    error('No image found. Build first.');
    process.exit(1);
  }
  info(`Image: ${image.name} (${formatBytes(image.stat.size)})`);

  // Select device.
  const devices = getBlockDevices();
  if (devices.length === 0) {
    error('No suitable devices found.');
    process.exit(1);
  }
  const device = await selectDevice(devices);
  if (!device) process.exit(0);

  // Get hostname.
  const hostnameInput = await prompt(`Hostname (default: ${DEFAULT_HOSTNAME}): `);
  const hostname = hostnameInput.trim() || DEFAULT_HOSTNAME;

  // Backup existing data partition if present.
  let backupDir = null;
  if (hasDataPartition(device)) {
    const doBackup = await prompt('Backup /data before flashing? (Y/n): ');
    if (doBackup.toLowerCase() !== 'n') {
      backupDir = backupDataPartition(device);
    }
  }

  // Get WiFi credentials (skip if restoring backup).
  let wifiCreds = null;
  if (!backupDir) {
    wifiCreds = await getWifiCredentials(WIFI_CREDS_FILE);
  }

  // Flash.
  const bmapPath = image.path.replace('.wic.gz', '.wic.bmap');
  await flashImage(image.path, device, {
    bmapPath: existsSync(bmapPath) ? bmapPath : null,
  });

  // Post-flash setup.
  await injectSSHKey(device, config.ssh_key_path, USERNAME, USER_UID);
  await setHostname(device, hostname);

  if (wifiCreds) {
    await injectWifiCredentials(device, wifiCreds.ssid, wifiCreds.password);
  }
  if (backupDir) {
    restoreDataPartition(device, backupDir);
    cleanupBackup(backupDir);
  }

  log('');
  success('Flash complete!');
  info(`Login: ssh ${USERNAME}@${hostname}.local`);
}

main().catch(err => {
  error(err.message);
  process.exit(1);
});
```

## 5. WiFi Credentials File (Optional)

Create `wifi-creds.local` to avoid typing WiFi credentials each flash:

```json
{
  "ssid": "MyNetwork",
  "password": "secret123"
}
```

Add to `.gitignore`:
```
wifi-creds.local
```

## 6. Update Submodule

To pull updates from sparkle-duck-shared:

```bash
cd yocto/src/pi-base
git fetch origin
git checkout v1.0.0   # or specific tag/commit
cd ../..
git add src/pi-base
git commit -m "Update pi-base to v1.0.0"
```

## Partition Layout

The A/B partition layout:

| Partition | Label    | Mount Point | Purpose                              |
|-----------|----------|-------------|--------------------------------------|
| 1         | boot     | /boot       | Kernel, DTBs, config.txt, cmdline.txt |
| 2         | rootfs_a | /           | Active root filesystem               |
| 3         | rootfs_b | (inactive)  | Backup root filesystem for updates   |
| 4         | data     | /data       | Persistent data (WiFi, logs, config) |

## OTA Updates

Once running, update the device over the network:

```bash
# On the device:
ab-boot-manager status          # Show current slot
ab-update /tmp/rootfs.ext4.gz   # Flash to inactive slot
sudo reboot                     # Boot into new image
```

The `/data` partition is preserved across updates - WiFi credentials survive.
