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

- [Inky Soup](https://github.com/yourorg/inky-soup) - Web-based e-ink display system for Pimoroni Inky Impression
- [Sparkle Duck (Dirt Sim)](https://github.com/yourorg/sparkle-duck) - Flight simulator display system

## Structure

```
sparkle-duck-shared/
├── yocto/
│   └── meta-pi-base/          # Shared Yocto layer
│       ├── wic/               # Partition layouts
│       ├── recipes-support/   # Infrastructure packages
│       └── classes/           # Common image configuration
├── scripts/
│   └── lib/                   # Shared flash script utilities
└── docs/                      # Integration guides
```

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

**Current:** Planning phase
**Next:** Extract common code from inky-soup and sparkle-duck

## Contributing

This is currently a private infrastructure project. Contributions are not being accepted at this time.
