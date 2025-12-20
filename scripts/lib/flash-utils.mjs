/**
 * Flash utilities for flash scripts.
 * Handles device discovery, image finding, and flashing operations.
 */

import { execSync, spawn } from 'child_process';
import { existsSync, readdirSync, statSync } from 'fs';
import { join } from 'path';
import { colors, log, info, warn, error, prompt } from './cli-utils.mjs';

/**
 * Get list of block devices suitable for flashing.
 * Returns removable devices and excludes the system disk.
 * @returns {Array<{device: string, size: string, model: string, transport: string, removable: boolean}>}
 */
export function getBlockDevices() {
  try {
    const output = execSync('lsblk -d -o NAME,SIZE,TYPE,RM,TRAN,MODEL -J', {
      encoding: 'utf-8',
    });
    const data = JSON.parse(output);

    return data.blockdevices
      .filter(dev => {
        // Only disk types.
        if (dev.type !== 'disk') return false;
        // Skip loop devices.
        if (dev.name.startsWith('loop')) return false;
        // Skip nvme (usually system disk).
        if (dev.name.startsWith('nvme')) return false;
        // Prefer removable (RM=1) or USB transport.
        return dev.rm === true || dev.rm === '1' || dev.tran === 'usb';
      })
      .map(dev => ({
        device: `/dev/${dev.name}`,
        size: dev.size,
        model: dev.model || 'Unknown',
        transport: dev.tran || 'unknown',
        removable: dev.rm === true || dev.rm === '1',
      }));
  } catch (err) {
    error(`Failed to list block devices: ${err.message}`);
    return [];
  }
}

/**
 * Find the latest image file matching a pattern.
 * @param {string} imageDir - Directory to search in.
 * @param {string} suffix - File suffix to match (e.g., '.wic.gz').
 * @param {string[]} preferredNames - Preferred image names in order of priority.
 * @returns {{name: string, path: string, stat: object}|null}
 */
export function findLatestImage(imageDir, suffix = '.wic.gz', preferredNames = []) {
  if (!existsSync(imageDir)) {
    return null;
  }

  const files = readdirSync(imageDir)
    .filter(f => f.endsWith(suffix) && !f.includes('->'))
    .map(f => ({
      name: f,
      path: join(imageDir, f),
      // Follow symlinks to get real file for mtime.
      stat: statSync(join(imageDir, f)),
    }))
    .sort((a, b) => b.stat.mtimeMs - a.stat.mtimeMs);

  // Check preferred names in order.
  for (const preferred of preferredNames) {
    const found = files.find(f => f.name === preferred);
    if (found) {
      return found;
    }
  }

  return files[0] || null;
}

/**
 * Check if bmaptool is available.
 * @returns {boolean} True if bmaptool is installed.
 */
export function hasBmaptool() {
  try {
    execSync('which bmaptool', { encoding: 'utf-8', stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

/**
 * Flash an image to a device.
 * @param {string} imagePath - Path to the .wic.gz image.
 * @param {string} device - Target device (e.g., /dev/sdb).
 * @param {object} options - Flash options.
 * @param {boolean} options.dryRun - If true, only show what would happen.
 * @param {boolean} options.skipConfirm - If true, skip confirmation prompt.
 * @param {string} options.bmapPath - Path to .bmap file (optional).
 */
export async function flashImage(imagePath, device, options = {}) {
  const { dryRun = false, skipConfirm = false, bmapPath = null } = options;
  const useBmap = hasBmaptool() && bmapPath && existsSync(bmapPath);

  log('');
  if (dryRun) {
    log(`${colors.bold}${colors.yellow}═══════════════════════════════════════════════════${colors.reset}`);
    log(`${colors.bold}${colors.yellow}  DRY RUN - No changes will be made${colors.reset}`);
    log(`${colors.bold}${colors.yellow}═══════════════════════════════════════════════════${colors.reset}`);
  } else {
    log(`${colors.bold}${colors.red}═══════════════════════════════════════════════════${colors.reset}`);
    log(`${colors.bold}${colors.red}  WARNING: This will ERASE ALL DATA on ${device}${colors.reset}`);
    log(`${colors.bold}${colors.red}═══════════════════════════════════════════════════${colors.reset}`);
  }
  log('');

  info(`Image: ${imagePath}`);
  info(`Target: ${device}`);
  info(`Method: ${useBmap ? 'bmaptool (fast)' : 'dd (slower)'}`);
  log('');

  if (dryRun) {
    info('Dry run complete. Would execute:');
    log('');
    if (useBmap) {
      log(`  sudo umount ${device}* 2>/dev/null || true`);
      log(`  sudo bmaptool copy --bmap "${bmapPath}" "${imagePath}" "${device}"`);
    } else {
      log(`  sudo umount ${device}* 2>/dev/null || true`);
      log(`  gunzip -c "${imagePath}" | sudo dd of="${device}" bs=4M status=progress conv=fsync`);
    }
    log(`  sync`);
    log('');
    return;
  }

  if (!skipConfirm) {
    const confirm = await prompt(`Type "${device}" to confirm: `);
    if (confirm !== device) {
      error('Confirmation failed. Aborting.');
      process.exit(1);
    }
  }

  log('');

  // Unmount any partitions on the device.
  try {
    info('Unmounting any mounted partitions...');
    execSync(`sudo umount ${device}* 2>/dev/null || true`, { stdio: 'inherit' });
  } catch {
    // Ignore unmount errors.
  }

  if (useBmap) {
    // Use bmaptool for faster flashing.
    const cmd = `sudo bmaptool copy --bmap "${bmapPath}" "${imagePath}" "${device}"`;
    info(`Running: ${cmd}`);
    log('');

    const proc = spawn('sudo', [
      'bmaptool', 'copy',
      '--bmap', bmapPath,
      imagePath,
      device,
    ], { stdio: 'inherit' });

    await new Promise((resolve, reject) => {
      proc.on('close', code => {
        if (code === 0) {
          resolve();
        } else {
          reject(new Error(`bmaptool exited with code ${code}`));
        }
      });
    });
  } else {
    // Fall back to dd.
    const cmd = `gunzip -c "${imagePath}" | sudo dd of="${device}" bs=4M status=progress conv=fsync`;
    info(`Running: ${cmd}`);
    log('');

    const proc = spawn('sh', ['-c', cmd], { stdio: 'inherit' });

    await new Promise((resolve, reject) => {
      proc.on('close', code => {
        if (code === 0) {
          resolve();
        } else {
          reject(new Error(`dd exited with code ${code}`));
        }
      });
    });
  }

  // Final sync to ensure all writes are flushed.
  info('Syncing...');
  execSync('sync', { stdio: 'inherit' });
}

/**
 * Display available devices for selection.
 * @param {Array} devices - Array of device objects from getBlockDevices().
 */
export function displayDevices(devices) {
  log(`${colors.bold}Available devices:${colors.reset}`);
  log('');
  devices.forEach((dev, i) => {
    const rmBadge = dev.removable ? `${colors.green}[removable]${colors.reset}` : '';
    log(`  ${colors.cyan}${i + 1})${colors.reset} ${dev.device}  ${dev.size}  ${dev.model}  ${rmBadge}`);
  });
  log('');
}

/**
 * Interactively select a device from the list.
 * @param {Array} devices - Array of device objects from getBlockDevices().
 * @returns {Promise<string|null>} Selected device path, or null if cancelled.
 */
export async function selectDevice(devices) {
  displayDevices(devices);

  const choice = await prompt(`Select device (1-${devices.length}) or 'q' to quit: `);

  if (choice.toLowerCase() === 'q') {
    info('Aborted.');
    return null;
  }

  const index = parseInt(choice, 10) - 1;
  if (isNaN(index) || index < 0 || index >= devices.length) {
    error('Invalid selection.');
    return null;
  }

  return devices[index].device;
}
