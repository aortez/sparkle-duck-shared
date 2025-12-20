/**
 * Partition utilities for flash scripts.
 * Handles mounting, backup, and restore of partitions.
 */

import { execSync } from 'child_process';
import { existsSync, readdirSync, mkdtempSync, rmdirSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { log, info, success, warn, TEMP_PREFIX } from './cli-utils.mjs';

/**
 * Check if the device has a data partition (partition 4) with content.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @returns {boolean} True if data partition exists.
 */
export function hasDataPartition(device) {
  const dataPartition = `${device}4`;
  try {
    // Check if partition exists.
    execSync(`test -b ${dataPartition}`, { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

/**
 * Backup /data from the local disk's partition 4 before flashing.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @returns {string|null} The backup directory path, or null on failure.
 */
export function backupDataPartition(device) {
  const dataPartition = `${device}4`;
  const backupDir = mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}data-backup-`));
  const mountPoint = mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}data-mount-`));

  try {
    info(`Backing up data partition from ${dataPartition}...`);

    // Mount the data partition.
    execSync(`sudo mount ${dataPartition} ${mountPoint}`, { stdio: 'pipe' });

    // Copy contents to backup dir, preserving ownership info in extended attributes.
    // We use --fake-super to store ownership as xattrs since we're not root.
    execSync(`sudo rsync -a --fake-super ${mountPoint}/ ${backupDir}/`, { stdio: 'pipe' });

    // Fix ownership of backup dir itself so we can list it.
    execSync(`sudo chown $(id -u):$(id -g) ${backupDir}`, { stdio: 'pipe' });

    // Verify we got something useful (more than just lost+found).
    const files = readdirSync(backupDir).filter(f => f !== 'lost+found');
    if (files.length === 0) {
      info('Data partition is empty (nothing to backup)');
      rmdirSync(backupDir, { recursive: true });
      return null;
    }

    success(`Backed up ${files.length} items from data partition`);
    return backupDir;

  } catch (err) {
    warn(`Backup failed: ${err.message}`);
    try {
      rmdirSync(backupDir, { recursive: true });
    } catch {
      // Ignore cleanup errors.
    }
    return null;

  } finally {
    // Always unmount.
    try {
      execSync(`sudo umount ${mountPoint} 2>/dev/null || true`, { stdio: 'pipe' });
      rmdirSync(mountPoint);
    } catch {
      // Ignore cleanup errors.
    }
  }
}

/**
 * Restore backed up data to the data partition on the flashed device.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @param {string} backupDir - Path to the backup directory.
 * @param {boolean} dryRun - If true, only show what would happen.
 * @returns {boolean} True if successful.
 */
export function restoreDataPartition(device, backupDir, dryRun = false) {
  const dataPartition = `${device}4`;

  log('');
  info('Restoring data to new image...');

  if (dryRun) {
    log(`  Would mount ${dataPartition}`);
    log(`  Would restore data from ${backupDir}`);
    log(`  Would unmount`);
    return true;
  }

  const mountPoint = mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}data-restore-`));

  try {
    // Mount the data partition.
    info(`Mounting ${dataPartition}...`);
    execSync(`sudo mount ${dataPartition} ${mountPoint}`, { stdio: 'pipe' });

    // Restore the backup, using --fake-super to restore ownership from xattrs.
    info('Copying backed up data...');
    execSync(`sudo rsync -a --fake-super ${backupDir}/ ${mountPoint}/`, { stdio: 'pipe' });

    success('Data restored!');
    return true;

  } catch (err) {
    warn(`Restore failed: ${err.message}`);
    return false;

  } finally {
    // Always try to unmount and clean up.
    try {
      info('Unmounting data partition...');
      execSync(`sudo umount ${mountPoint}`, { stdio: 'pipe' });
      rmdirSync(mountPoint);
    } catch (err) {
      warn(`Cleanup warning: ${err.message}`);
    }
  }
}

/**
 * Clean up backup directory.
 * @param {string} backupDir - Path to the backup directory.
 */
export function cleanupBackup(backupDir) {
  if (backupDir) {
    try {
      execSync(`rm -rf ${backupDir}`, { stdio: 'pipe' });
    } catch {
      // Ignore cleanup errors.
    }
  }
}

/**
 * Set hostname for the device by writing to boot partition.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @param {string} hostname - The hostname to set.
 * @param {boolean} dryRun - If true, only show what would happen.
 */
export async function setHostname(device, hostname, dryRun = false) {
  const bootPartition = `${device}1`;

  log('');
  info('Setting device hostname...');

  if (dryRun) {
    log(`  Would mount ${bootPartition}`);
    log(`  Would write hostname "${hostname}" to /boot/hostname.txt`);
    log(`  Would unmount`);
    return;
  }

  // Create temporary mount point.
  const mountPoint = mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}boot-`));

  try {
    // Mount the boot partition.
    info(`Mounting ${bootPartition}...`);
    execSync(`sudo mount ${bootPartition} ${mountPoint}`, { stdio: 'pipe' });

    // Write the hostname.
    const hostnameFilePath = join(mountPoint, 'hostname.txt');
    info(`Writing hostname "${hostname}" to hostname.txt...`);
    execSync(`echo '${hostname}' | sudo tee ${hostnameFilePath} > /dev/null`, { stdio: 'pipe' });
    execSync(`sudo chmod 644 ${hostnameFilePath}`, { stdio: 'pipe' });

    success(`Hostname set to: ${hostname}`);

  } finally {
    // Always try to unmount and clean up.
    try {
      info('Unmounting...');
      execSync(`sudo umount ${mountPoint}`, { stdio: 'pipe' });
      rmdirSync(mountPoint);
    } catch (err) {
      warn(`Cleanup warning: ${err.message}`);
    }
  }
}
