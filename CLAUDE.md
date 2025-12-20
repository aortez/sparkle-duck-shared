# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a shared infrastructure repository for Raspberry Pi projects in the Sparkle Duck family (Dirt Sim, Inky Soup, etc.). It provides:
- A shared Yocto layer (`meta-pi-base`) with common recipes for A/B updates
- Flash script utilities (JavaScript ES modules)
- Documentation for integration into downstream projects

## Integration Model

This repository is designed to be integrated as a **git submodule** into downstream projects:

```bash
# In downstream project
cd yocto/src/  # or yocto/sources/
git submodule add https://github.com/yourorg/sparkle-duck-shared.git pi-base
```

Downstream projects reference this repo's Yocto layer in their `bblayers.conf`.

## Repository Structure

```
yocto/meta-pi-base/
├── conf/layer.conf              # Layer configuration
├── wic/sdimage-ab.wks.in        # Parameterized A/B partition layout
└── recipes-support/
    ├── ab-boot/                 # A/B boot slot manager (ab-boot-manager, ab-update)
    ├── persistent-data/         # /data partition mount and NetworkManager bind
    └── hostname-setup/          # Set hostname from /boot/hostname.txt

scripts/lib/
├── index.mjs                    # Re-exports all utilities
├── cli-utils.mjs                # Colors, logging, prompt, formatBytes
├── config-utils.mjs             # Load/save .flash-config.json
├── ssh-utils.mjs                # SSH key discovery, reading, injection
├── partition-utils.mjs          # Mount, backup/restore data partition, hostname
├── flash-utils.mjs              # Device discovery, image finding, flash operation
└── wifi-utils.mjs               # WiFi credential generation and injection
```

## Yocto Recipes

### ab-boot-manager
Provides `ab-boot-manager` and `ab-update` scripts for A/B partition management:
- `ab-boot-manager current` - Show current boot slot (a or b)
- `ab-boot-manager inactive` - Show inactive slot
- `ab-boot-manager inactive-device` - Get inactive partition device path
- `ab-boot-manager switch {a|b}` - Switch boot to specified slot
- `ab-boot-manager status` - Show full status
- `ab-update <rootfs.ext4.gz>` - Flash image to inactive slot and switch

### persistent-data
Mounts `/data` partition (partition 4) and bind-mounts NetworkManager connections so WiFi credentials survive A/B updates.

### hostname-setup
Reads `/boot/hostname.txt` at boot and sets the system hostname. Override default with `HOSTNAME_DEFAULT` variable in your image recipe.

## WKS Partition Layout

The `sdimage-ab.wks.in` file requires these variables:
- `BOOT_DEVICE` - Required: "sda" for USB boot, "mmcblk0" for SD card
- `BOOT_SIZE` - Optional: Boot partition size in MB (default: 150)
- `ROOTFS_SIZE` - Optional: Root filesystem size in MB (default: 800)
- `DATA_SIZE` - Optional: Data partition size in MB (default: 100)

## JavaScript Flash Utilities

Import utilities in your project's flash script:

```javascript
import {
  colors, log, info, success, warn, error, prompt, formatBytes,
  loadConfig, saveConfig,
  findSshKeys, readSshKey, configureSSHKey, injectSSHKey,
  hasDataPartition, backupDataPartition, restoreDataPartition, cleanupBackup, setHostname,
  getBlockDevices, findLatestImage, hasBmaptool, flashImage, displayDevices, selectDevice,
  generateWifiConnection, loadWifiCredsFile, getWifiCredentials, injectWifiCredentials,
} from '../path/to/pi-base/scripts/lib/index.mjs';
```

## What Is Shared vs. Project-Specific

**Shared (in this repo):**
- A/B boot management recipes
- Persistent data partition support
- Hostname setup from boot partition
- A/B partition layout template (WKS)
- Flash script utilities (device discovery, SSH injection, WiFi injection, backup/restore)

**Project-Specific (in downstream repos):**
- `MACHINE` selection (raspberrypi0-2w-64, raspberrypi5, etc.)
- `BOOT_DEVICE` setting ("mmcblk0" vs "sda")
- Application packages and services
- Top-level image recipe
- Top-level flash.mjs (imports from shared lib, adds project-specific paths/defaults)

## Design References

See `docs/design.md` for the full architectural design document.
