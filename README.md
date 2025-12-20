# Sparkle Duck Shared Infrastructure

Shared Yocto build infrastructure and flash utilities for Raspberry Pi projects in the Sparkle Duck family (Dirt Sim, Inky Soup, etc.).

## Features

- **A/B Partition Layout** - Safe OTA updates with rollback support
- **Persistent Data Partition** - WiFi credentials survive reflashes and updates
- **WiFi Credential Injection** - Configure networking at flash time
- **Hostname Setup** - Device-specific hostnames via /boot/hostname.txt
- **Flash Script Utilities** - Shared JavaScript library for SD card flashing
- **NetworkManager + Avahi** - WiFi management and mDNS discovery

## Projects Using This

- [Inky Soup](https://github.com/yourorg/inky-soup) - Web-based e-ink display system
- [Sparkle Duck (Dirt Sim)](https://github.com/yourorg/sparkle-duck) - Dirt simulation

## Structure

```
sparkle-duck-shared/
├── yocto/
│   └── meta-pi-base/              # Shared Yocto layer
│       ├── conf/layer.conf        # Layer configuration
│       ├── wic/sdimage-ab.wks.in  # Parameterized A/B partition layout
│       └── recipes-support/
│           ├── ab-boot/           # A/B boot slot manager
│           ├── persistent-data/   # /data partition mount
│           └── hostname-setup/    # Hostname from /boot/hostname.txt
├── scripts/
│   └── lib/                       # Shared flash script utilities (ES modules)
└── docs/                          # Integration guides
```

## Yocto Recipes

### ab-boot-manager
Manages A/B boot partitions for safe OTA updates:
- `ab-boot-manager status` - Show current/inactive slots
- `ab-update <rootfs.ext4.gz>` - Flash to inactive slot and switch boot

### persistent-data
Mounts `/data` partition and bind-mounts NetworkManager connections so WiFi credentials persist across updates.

### hostname-setup
Sets hostname from `/boot/hostname.txt` at boot time.

## Integration

Add as a git submodule to your project:

```bash
cd your-project/yocto/src/
git submodule add https://github.com/yourorg/sparkle-duck-shared.git pi-base
```

See [docs/integration.md](docs/integration.md) for detailed setup instructions.

## License

This software is source-available for personal, educational, and evaluation use.
Commercial use requires a separate license.

See [LICENSE](LICENSE) for full terms.

For commercial licensing inquiries: [Open an issue](https://github.com/aortez/sparkle-duck-shared/issues)

## Development Status

**Current:** Initial extraction complete - A/B boot, persistent data, flash utilities
**Next:** Integration testing with inky-soup and sparkle-duck
