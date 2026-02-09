/**
 * Partition utilities for flash scripts.
 * Handles mounting, backup, and restore of partitions.
 */

import { execSync } from 'child_process';
import { readdirSync, mkdtempSync, rmdirSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { log, info, success, warn, TEMP_PREFIX } from './cli-utils.mjs';

function hasCommand(cmd) {
  try {
    execSync(`command -v ${cmd} >/dev/null 2>&1`, { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

function requireCommand(cmd, installHint) {
  if (hasCommand(cmd)) {
    return;
  }

  const hint = installHint ? ` ${installHint}` : '';
  throw new Error(`Missing required command: ${cmd}.${hint}`);
}

function runAllowExitCodes(cmd, allowedExitCodes = [0]) {
  try {
    execSync(cmd, { stdio: 'pipe' });
    return 0;
  } catch (err) {
    const code = typeof err.status === 'number' ? err.status : null;
    if (code !== null && allowedExitCodes.includes(code)) {
      return code;
    }

    const stderr = err && err.stderr ? String(err.stderr).trim() : '';
    const details = stderr ? `\n${stderr}` : '';
    throw new Error(`Command failed: ${cmd}${details}`);
  }
}

function refreshPartitionTable(device, partitionNumber = null) {
  // Best-effort: ask the kernel to re-read the partition table and refresh nodes.
  if (hasCommand('partprobe')) {
    try {
      execSync(`sudo partprobe ${device}`, { stdio: 'pipe' });
    } catch {
      // Ignore - we'll try partx below.
    }
  }

  if (partitionNumber !== null && hasCommand('partx')) {
    try {
      execSync(`sudo partx --update --nr ${partitionNumber} ${device}`, { stdio: 'pipe' });
    } catch {
      // Ignore - the kernel may already be updated.
    }
  }

  if (hasCommand('udevadm')) {
    try {
      execSync('sudo udevadm settle', { stdio: 'pipe' });
    } catch {
      // Ignore.
    }
  }
}

/**
 * Get a partition device path for a disk device and partition number.
 * Handles disks whose names end in digits (e.g. /dev/mmcblk0p4, /dev/nvme0n1p4).
 * @param {string} device - The disk device path (e.g. /dev/sdb, /dev/mmcblk0).
 * @param {number} partitionNumber - Partition number (1-based).
 * @returns {string} Partition device path.
 */
export function getPartitionDevice(device, partitionNumber) {
  if (!device || typeof device !== 'string') {
    throw new Error('device is required');
  }
  const part = Number(partitionNumber);
  if (!Number.isInteger(part) || part <= 0) {
    throw new Error(`Invalid partitionNumber: ${partitionNumber}`);
  }

  const needsP = /\d$/.test(device);
  return needsP ? `${device}p${part}` : `${device}${part}`;
}

/**
 * Check if the device has a data partition (partition 4) with content.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @returns {boolean} True if data partition exists.
 */
export function hasDataPartition(device) {
  const dataPartition = getPartitionDevice(device, 4);
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
  const dataPartition = getPartitionDevice(device, 4);
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
  const dataPartition = getPartitionDevice(device, 4);

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
 * Grow the data partition (partition 4) to fill the disk, leaving a percentage unallocated,
 * and then expand the ext4 filesystem to match.
 * @param {string} device - The disk device path (e.g., /dev/sdb).
 * @param {number} freePercent - Percentage of total disk space to leave unallocated.
 * @param {boolean} dryRun - If true, only show what would happen.
 */
export function growDataPartition(device, freePercent = 10, dryRun = false) {
  const partitionNumber = 4;
  const dataPartition = getPartitionDevice(device, partitionNumber);

  log('');
  info(`Growing data partition to fill disk (leave ${freePercent}% unallocated)...`);

  if (dryRun) {
    log(`  Would run: sudo growpart --free-percent=${freePercent} ${device} ${partitionNumber}`);
    log(`  Would run: sudo partprobe ${device} || true`);
    log(`  Would run: sudo partx --update --nr ${partitionNumber} ${device} || true`);
    log(`  Would run: sudo e2fsck -f -p ${dataPartition}`);
    log(`  Would run: sudo resize2fs ${dataPartition}`);
    return;
  }

  requireCommand('growpart', 'Install cloud-utils-growpart (e.g. `sudo apt-get install cloud-utils-growpart`).');
  requireCommand('e2fsck', 'Install e2fsprogs (e.g. `sudo apt-get install e2fsprogs`).');
  requireCommand('resize2fs', 'Install e2fsprogs (e.g. `sudo apt-get install e2fsprogs`).');

  // Ensure no partitions on the device are mounted (some desktops may automount them).
  try {
    execSync(`sudo umount ${device}* 2>/dev/null || true`, { stdio: 'pipe' });
  } catch {
    // Ignore.
  }

  refreshPartitionTable(device, partitionNumber);

  // growpart returns exit code 1 for NOCHANGE, which is not an error.
  const growExit = runAllowExitCodes(
    `sudo growpart --free-percent=${freePercent} ${device} ${partitionNumber}`,
    [0, 1],
  );
  if (growExit === 1) {
    info('Data partition already at maximum size (within free-percent target).');
  }

  refreshPartitionTable(device, partitionNumber);

  // e2fsck returns 1 (fixed) or 2 (fixed, reboot recommended) in some cases; treat those as success.
  runAllowExitCodes(`sudo e2fsck -f -p ${dataPartition}`, [0, 1, 2]);
  execSync(`sudo resize2fs ${dataPartition}`, { stdio: 'pipe' });

  success('Data partition resized.');
}

/**
 * Set hostname for the device by writing to boot partition.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @param {string} hostname - The hostname to set.
 * @param {boolean} dryRun - If true, only show what would happen.
 */
export async function setHostname(device, hostname, dryRun = false) {
  const bootPartition = getPartitionDevice(device, 1);

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
