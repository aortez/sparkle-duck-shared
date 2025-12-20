# Pi Base - Shared Infrastructure Design

**Date:** December 19, 2025
**Status:** Planning
**Projects:** inky-soup, sparkle-duck (future: others?)

## Motivation

Both inky-soup and sparkle-duck share ~90% of their Yocto infrastructure:
- A/B partition layout for safe updates
- Persistent `/data` partition for WiFi credentials
- NetworkManager + avahi setup
- systemd-based services
- Flash script logic (SSH key injection, hostname setup, data backup/restore)

Currently, improvements made to one project aren't easily transferred to the other. This leads to:
- Duplicated effort
- Divergent implementations
- Harder maintenance

## Goal

Create a shared `pi-base` repository that provides:
1. **Yocto layer** (`meta-pi-base`) with common recipes
2. **Flash script utilities** (JavaScript library)
3. **Documentation** for both projects to use

## Architecture

### Repository Structure

```
pi-base/                                    # New standalone repo
├── README.md
├── LICENSE
├── package.json                            # For flash script utilities
├── yocto/
│   └── meta-pi-base/                       # Shared Yocto layer
│       ├── conf/
│       │   └── layer.conf
│       ├── wic/
│       │   └── sdimage-ab.wks.in          # Parameterized A/B layout
│       ├── recipes-support/
│       │   ├── persistent-data/            # WiFi credential persistence
│       │   └── hostname-setup/             # /boot/hostname.txt support
│       ├── recipes-connectivity/
│       │   └── network-base/               # NetworkManager + avahi
│       └── classes/
│           └── pi-base-image.bbclass       # Common image configuration
├── scripts/
│   ├── lib/
│   │   ├── flash-utils.mjs                # Shared flash functions
│   │   ├── partition-utils.mjs            # Mount/unmount helpers
│   │   └── ssh-utils.mjs                  # SSH key injection
│   └── README.md
└── docs/
    ├── integration.md                      # How to integrate into projects
    ├── customization.md                    # How to customize for specific devices
    └── flash-script-api.md                 # Flash utility API docs
```

### Integration via Git Submodules

Projects add pi-base as a git submodule:

```bash
# In inky-soup
cd yocto/src/
git submodule add https://github.com/yourorg/pi-base.git

# In sparkle-duck
cd yocto/sources/
git submodule add https://github.com/yourorg/pi-base.git
```

### What Gets Shared

**Yocto Recipes:**
- `persistent-data` - /data partition mount, bind-mount for NetworkManager
- `hostname-setup` - Set hostname from /boot/hostname.txt
- `network-base` - NetworkManager, nmtui, avahi-daemon, WiFi firmware
- `pi-base-image.bbclass` - Common IMAGE_INSTALL, systemd config

**WKS Template:**
- Parameterized A/B partition layout (boot, rootfs_a, rootfs_b, data)
- Variables: `BOOT_DEVICE`, `BOOT_SIZE`, `ROOTFS_SIZE`, `DATA_SIZE`

**Flash Script Utilities:**
- Device detection and selection
- SSH key discovery and injection
- Hostname configuration
- WiFi credential prompting and injection
- Data partition backup/restore
- bmaptool vs dd logic

**Documentation:**
- Integration guides
- API documentation
- Best practices

### What Stays Project-Specific

**Yocto:**
- `MACHINE` selection (raspberrypi0-2w vs raspberrypi5)
- Application packages (inky-soup-server vs sparkle-duck-ui)
- Device-specific configuration
- Image recipe (inherits from pi-base-image.bbclass)

**Flash Scripts:**
- Top-level flash.mjs (imports from pi-base/scripts/lib/)
- Project-specific image paths
- Device-specific defaults (BOOT_DEVICE, hostname)

**Build System:**
- init.sh (each project may clone different layers)
- package.json scripts
- CI/CD pipelines

## Example Integration

### Inky-soup Image Recipe

```bitbake
# inky-soup/yocto/src/meta-inky-soup/recipes-core/images/inky-soup-image.bb
require ${LAYERDIR_pi-base}/classes/pi-base-image.bbclass

DESCRIPTION = "Inky Soup e-ink display server"

# Device-specific settings
BOOT_DEVICE = "mmcblk0"
ROOTFS_SIZE = "800"

# Project-specific packages
IMAGE_INSTALL:append = " \
    inky-soup-server \
    python3-pillow \
    python3-inky \
"

# Use shared A/B layout
WKS_FILE = "sdimage-ab.wks"
```

### Inky-soup Flash Script

```javascript
// inky-soup/yocto/scripts/flash.mjs
import {
  findBlockDevices,
  findLatestImage,
  flashImage,
  injectSSHKey,
  setHostname,
  getWifiCredentials,
  injectWifiCredentials,
  backupDataPartition,
  restoreDataPartition,
} from '../src/pi-base/scripts/lib/flash-utils.mjs';

// Project-specific configuration
const IMAGE_DIR = '../build/poky/tmp/deploy/images/raspberrypi0-2w';
const BOOT_DEVICE = 'mmcblk0';
const DEFAULT_HOSTNAME = 'inky-soup';

async function main() {
  const image = findLatestImage(IMAGE_DIR);
  const devices = findBlockDevices();

  // ... device selection logic ...

  const wifiCreds = await getWifiCredentials('wifi-creds.local');

  // Use shared utilities
  await flashImage(image.path, targetDevice);
  await injectSSHKey(targetDevice, sshKeyPath);
  await setHostname(targetDevice, hostname);
  await injectWifiCredentials(targetDevice, wifiCreds);
}
```

### Sparkle-duck Image Recipe

```bitbake
# sparkle-duck/yocto/meta-dirtsim/recipes-core/images/dirtsim-image.bb
require ${LAYERDIR_pi-base}/classes/pi-base-image.bbclass

DESCRIPTION = "Sparkle Duck flight simulator display"

# Device-specific settings (Pi 5 boots from USB)
BOOT_DEVICE = "sda"
ROOTFS_SIZE = "800"

# Project-specific packages
IMAGE_INSTALL:append = " \
    sparkle-duck-ui \
    sparkle-duck-server \
"

WKS_FILE = "sdimage-ab.wks"
```

## Migration Plan

### Phase 1: Create pi-base Repository

1. Create new `pi-base` repository
2. Extract common code from inky-soup (most recent/clean)
3. Parameterize device-specific bits
4. Add documentation
5. Create initial release/tag

### Phase 2: Integrate into inky-soup

1. Add pi-base as submodule
2. Update bblayers.conf to include meta-pi-base
3. Refactor inky-soup-image.bb to inherit from pi-base-image.bbclass
4. Update flash.mjs to use shared utilities
5. Remove duplicated code from meta-inky-soup
6. Test build and flash
7. Commit and tag

### Phase 3: Integrate into sparkle-duck

1. Add pi-base as submodule
2. Update bblayers.conf
3. Refactor dirtsim-image.bb
4. Update flash scripts
5. Remove duplicated code from meta-dirtsim
6. Test build and flash
7. Commit and tag

### Phase 4: Ongoing Maintenance

- Improvements to pi-base benefit both projects
- Each project can pin to specific pi-base version
- Submit PRs to pi-base for enhancements
- Projects update pi-base submodule when ready

## Design Decisions

### Why Git Submodules?

**Pros:**
- Well understood in Yocto ecosystem
- Projects can pin to specific versions
- Easy to contribute changes back
- Works with existing git workflows

**Alternatives considered:**
- npm package: Mixing package managers is awkward
- Monorepo: Too tightly coupled, huge repo
- Copy-paste: No shared maintenance

### Parameterization Approach

Use Yocto variables for device-specific config:
- `BOOT_DEVICE` - sda vs mmcblk0
- `ROOTFS_SIZE` - Allow projects to customize
- `DATA_SIZE` - Allow projects to customize

The WKS file uses Yocto variable expansion:
```wks
part /boot --ondisk ${BOOT_DEVICE} --size ${BOOT_SIZE}
```

### Versioning Strategy

Use git tags for releases:
- `v1.0.0` - Initial stable release
- `v1.1.0` - Add feature X
- `v2.0.0` - Breaking change Y

Projects update submodule when ready:
```bash
cd yocto/src/pi-base
git checkout v1.1.0
cd ../..
git add src/pi-base
git commit -m "Update pi-base to v1.1.0"
```

## Open Questions

1. **Repository hosting:** GitHub public/private? Self-hosted?
2. **Naming:** `pi-base` vs `meta-pi-common` vs something else?
3. **License:** MIT? GPL? Same as parent projects?
4. **Flash script packaging:** ES modules via node? Require build step?
5. **Testing:** How to test pi-base changes against both projects?

## Next Steps

1. Review and approve this design
2. Create pi-base repository skeleton
3. Extract common code from inky-soup
4. Document integration process
5. Integrate into inky-soup (testing ground)
6. Integrate into sparkle-duck
7. Iterate and improve

## Benefits

**Short term:**
- Code deduplication
- Consistent behavior across projects
- Single place to fix bugs

**Long term:**
- Easy to add new Pi-based projects
- Community contributions to shared infrastructure
- Best practices codified in one place
- Faster iteration on both projects

## Success Criteria

- Both projects build and flash successfully with pi-base
- Improvements to pi-base automatically benefit both projects
- Adding a third project takes < 1 day of infrastructure work
- Flash scripts share > 80% of code via pi-base utilities
- Documentation allows new contributors to understand the architecture
