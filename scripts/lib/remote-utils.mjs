/**
 * Remote utilities for SSH operations and remote system interrogation.
 * Provides functions for SSH command execution, reachability checks, and remote system queries.
 */

import { basename } from 'path';
import { run, runCapture } from './command-utils.mjs';
import { colors, info, success, warn } from './cli-utils.mjs';

/**
 * Run a command on remote host via SSH and capture output.
 * @param {string} remoteTarget - user@host string.
 * @param {string} command - Command to execute.
 * @param {object} options - SSH options.
 * @param {number} options.timeout - Connect timeout in seconds (default: 5).
 * @param {object} options.execOptions - Additional execSync options.
 * @returns {string|null} Command output or null on error.
 */
export function ssh(remoteTarget, command, options = {}) {
  const { timeout = 5, execOptions = {} } = options;
  const sshCmd = `ssh -o ConnectTimeout=${timeout} -o BatchMode=yes ${remoteTarget} "${command}"`;
  return runCapture(sshCmd, execOptions);
}

/**
 * Run a command on remote host via SSH with inherited stdio (for progress).
 * @param {string} remoteTarget - user@host string.
 * @param {string} command - Command to execute.
 * @param {object} options - SSH options.
 * @param {number} options.timeout - Connect timeout in seconds (default: 10).
 * @returns {Promise<void>}
 */
export async function sshRun(remoteTarget, command, options = {}) {
  const { timeout = 10 } = options;
  return run('ssh', [
    '-o', `ConnectTimeout=${timeout}`,
    '-o', 'BatchMode=yes',
    remoteTarget,
    command,
  ]);
}

/**
 * Check if remote host is reachable via ping and SSH.
 * @param {string} remoteHost - Hostname or IP.
 * @param {string} remoteTarget - user@host for SSH check.
 * @returns {boolean} True if both ping and SSH succeed.
 */
export function checkRemoteReachable(remoteHost, remoteTarget) {
  info(`Checking if ${remoteHost} is reachable...`);

  const result = runCapture(`ping -c 1 -W 2 ${remoteHost}`);
  if (result === null) {
    return false;
  }

  // Also check SSH.
  const sshResult = ssh(remoteTarget, 'echo ok');
  return sshResult === 'ok';
}

/**
 * Get available space in remote directory (in bytes).
 * Uses -k flag for BusyBox compatibility (returns KB).
 * @param {string} remoteTarget - user@host string.
 * @param {string} remotePath - Path to check (default: /tmp).
 * @returns {number} Available space in bytes, or 0 on error.
 */
export function getRemoteTmpSpace(remoteTarget, remotePath = '/tmp') {
  // Use awk with escaped braces for SSH.
  const result = ssh(remoteTarget, `df -k ${remotePath} | tail -1 | awk '{ print \\$4 }'`);
  if (result) {
    const kb = parseInt(result, 10);
    if (!isNaN(kb)) {
      // Result is in KB, convert to bytes.
      return kb * 1024;
    }
  }
  return 0;
}

/**
 * Detect boot device on remote system.
 * @param {string} remoteTarget - user@host string.
 * @param {string} fallbackDevice - Device to return if detection fails (default: /dev/sda).
 * @returns {string} Device path (e.g., /dev/sda).
 */
export function getRemoteBootDevice(remoteTarget, fallbackDevice = '/dev/sda') {
  // Find what device / is mounted from.
  const result = ssh(remoteTarget, `mount | grep ' / ' | cut -d' ' -f1 | sed 's/[0-9]*$//'`);
  return result || fallbackDevice;
}

/**
 * Get remote system uptime in seconds.
 * Uses /proc/uptime which is reliable regardless of RTC/NTP status.
 * @param {string} remoteTarget - user@host string.
 * @returns {number} Uptime in seconds or -1 on error.
 */
export function getRemoteUptime(remoteTarget) {
  const result = ssh(remoteTarget, "cat /proc/uptime | cut -d' ' -f1");
  if (result) {
    const uptime = parseFloat(result);
    if (!isNaN(uptime)) {
      return uptime;
    }
  }
  return -1;
}

/**
 * Get remote system boot time (seconds since epoch).
 * Uses /proc/stat btime which is boot time in seconds since epoch.
 * Note: This can be unreliable on systems without RTC before NTP syncs.
 * @param {string} remoteTarget - user@host string.
 * @returns {number} Boot timestamp or 0 on error.
 */
export function getRemoteBootTime(remoteTarget) {
  const result = ssh(remoteTarget, "awk '/btime/ {print \\$2}' /proc/stat");
  if (result) {
    const btime = parseInt(result, 10);
    if (!isNaN(btime)) {
      return btime;
    }
  }
  return 0;
}

/**
 * Wait for remote system to reboot and come back online.
 * Verifies reboot by checking that uptime is low (< 60s) after system comes back.
 * This is more reliable than boot time on systems without RTC.
 * @param {string} remoteTarget - user@host string.
 * @param {string} remoteHost - Hostname for display.
 * @param {number} originalBootTime - Boot time before reboot (unused, kept for API compat).
 * @param {number} timeoutSec - Timeout in seconds (default: 120).
 * @returns {Promise<boolean>} True if reboot verified, false on timeout.
 */
export async function waitForReboot(remoteTarget, remoteHost, originalBootTime, timeoutSec = 120) {
  const startTime = Date.now();
  const timeoutMs = timeoutSec * 1000;
  const MAX_FRESH_UPTIME = 120; // Consider rebooted if uptime < 2 minutes.
  let dots = 0;
  let sawOffline = false;

  // Wait a bit for the system to go down.
  info('Waiting for shutdown...');

  while (Date.now() - startTime < timeoutMs) {
    process.stdout.write(`\r  Waiting${'.'.repeat(dots % 4).padEnd(4)} (${Math.floor((Date.now() - startTime) / 1000)}s)`);
    dots++;

    const sshResult = ssh(remoteTarget, 'echo ok');

    if (sshResult !== 'ok') {
      // System is offline.
      if (!sawOffline) {
        process.stdout.write('\r' + ' '.repeat(50) + '\r');
        info('System went offline...');
        sawOffline = true;
      }
    } else if (sawOffline) {
      // System came back - verify it actually rebooted by checking uptime.
      const uptime = getRemoteUptime(remoteTarget);
      if (uptime >= 0 && uptime < MAX_FRESH_UPTIME) {
        process.stdout.write('\r' + ' '.repeat(50) + '\r');
        success(`${remoteHost} is back online!`);
        info(`Uptime: ${uptime.toFixed(1)}s (freshly rebooted)`);
        return true;
      } else {
        // High uptime - didn't actually reboot!
        process.stdout.write('\r' + ' '.repeat(50) + '\r');
        warn(`System responded but uptime is ${uptime.toFixed(1)}s - reboot may have failed!`);
        return false;
      }
    }

    await new Promise(resolve => setTimeout(resolve, 2000));
  }

  process.stdout.write('\r' + ' '.repeat(50) + '\r');

  // Final check - maybe it rebooted quickly before we noticed it went offline.
  const uptime = getRemoteUptime(remoteTarget);
  if (uptime >= 0 && uptime < MAX_FRESH_UPTIME) {
    success(`${remoteHost} is back online!`);
    info(`Uptime: ${uptime.toFixed(1)}s (freshly rebooted)`);
    return true;
  }

  warn(`Timeout waiting for reboot after ${timeoutSec}s`);
  if (uptime >= 0) {
    console.error(`${colors.red}✗${colors.reset} Uptime is ${uptime.toFixed(1)}s - reboot did NOT happen!`);
  }
  return false;
}

/**
 * Transfer image to remote host via scp.
 * @param {string} imagePath - Local image path.
 * @param {string} checksum - SHA256 checksum.
 * @param {string} remoteTarget - user@host string.
 * @param {string} remoteTmp - Remote temp directory (default: /tmp).
 * @param {boolean} dryRun - If true, skip actual transfer.
 * @returns {Promise<{remoteImagePath: string, remoteChecksumPath: string}>}
 */
export async function transferImage(imagePath, checksum, remoteTarget, remoteTmp = '/tmp', dryRun = false) {
  const imageName = basename(imagePath);
  const remoteImagePath = `${remoteTmp}/${imageName}`;
  const remoteChecksumPath = `${remoteTmp}/${imageName}.sha256`;

  info(`Source: ${imageName}`);
  info(`Target: ${remoteTarget}:${remoteImagePath}`);
  console.log('');

  if (dryRun) {
    console.log(`${colors.yellow}DRY RUN - would execute:${colors.reset}`);
    console.log(`  scp ${imagePath} ${remoteTarget}:${remoteImagePath}`);
    console.log('');
    return { remoteImagePath, remoteChecksumPath };
  }

  // Transfer the image with progress.
  await run('scp', [
    '-o', 'ConnectTimeout=10',
    '-o', 'BatchMode=yes',
    imagePath,
    `${remoteTarget}:${remoteImagePath}`,
  ]);

  success('Image transferred!');

  // Write checksum file on remote.
  info('Writing checksum file...');
  ssh(remoteTarget, `echo '${checksum}  ${imageName}' > ${remoteChecksumPath}`);

  return { remoteImagePath, remoteChecksumPath };
}

/**
 * Verify checksum of remote file.
 * @param {string} remoteImagePath - Remote image path.
 * @param {string} remoteChecksumPath - Remote checksum file path.
 * @param {string} remoteTarget - user@host string.
 * @returns {boolean} True if checksum matches.
 */
export function verifyRemoteChecksum(remoteImagePath, remoteChecksumPath, remoteTarget) {
  info('Verifying checksum on Pi...');

  // Extract directory from checksum path to run command in correct location.
  const remoteDir = remoteChecksumPath.substring(0, remoteChecksumPath.lastIndexOf('/'));
  const result = ssh(remoteTarget, `cd ${remoteDir} && sha256sum -c ${basename(remoteChecksumPath)}`);

  if (result && result.includes('OK')) {
    success('Checksum verified!');
    return true;
  }

  console.error(`${colors.red}✗${colors.reset} Checksum verification failed!`);
  return false;
}
