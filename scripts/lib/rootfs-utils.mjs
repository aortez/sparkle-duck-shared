/**
 * Rootfs utilities for image preparation and remote flashing.
 * Provides functions for extracting, customizing, and flashing root filesystem images.
 */

import { createHash } from 'crypto';
import { createReadStream } from 'fs';
import { execSync } from 'child_process';
import { existsSync, readFileSync, mkdtempSync, unlinkSync, rmSync } from 'fs';
import { join, basename } from 'path';
import { tmpdir } from 'os';
import { sshRun } from './remote-utils.mjs';
import { runCapture } from './command-utils.mjs';
import { colors, info, success, warn, error, prompt, TEMP_PREFIX } from './cli-utils.mjs';

/**
 * Calculate SHA256 checksum of a file.
 * @param {string} filePath - Path to file.
 * @returns {Promise<string>} Hex checksum.
 */
export async function calculateChecksum(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(filePath);

    stream.on('data', data => hash.update(data));
    stream.on('end', () => resolve(hash.digest('hex')));
    stream.on('error', reject);
  });
}

/**
 * Extract rootfs from WIC image and inject SSH key.
 * For A/B updates, we only need the rootfs partition.
 * @param {string} imagePath - Path to .wic.gz image.
 * @param {object} config - Configuration object with ssh_key_path.
 * @param {string} username - Username for SSH key injection (e.g., 'dirtsim', 'inky').
 * @param {string} workingDirectory - Directory for temp files (optional, uses tmpdir).
 * @returns {Promise<{preparedRootfsPath: string, workDir: string}>}
 */
export async function prepareRootfs(imagePath, config, username, workingDirectory = null) {
  const workDir = workingDirectory || mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}rootfs-`));
  const wicPath = join(workDir, 'image.wic');
  const rootfsRaw = join(workDir, 'rootfs.ext4');
  const mountPoint = join(workDir, 'mnt');
  const preparedRootfsPath = join(workDir, 'rootfs.ext4.gz');

  let activeLoopDevice = null;
  let activeMountPoint = null;

  try {
    // Decompress image.
    info('Decompressing image...');
    execSync(`gunzip -c "${imagePath}" > "${wicPath}"`, { stdio: 'pipe' });

    // Set up loop device with partition scanning.
    info('Setting up loop device...');
    const loopDevice = execSync(`sudo losetup -fP --show "${wicPath}"`, {
      encoding: 'utf-8',
      stdio: 'pipe',
    }).trim();
    activeLoopDevice = loopDevice;

    try {
      // Extract partition 2 (rootfs_a) - this is what we'll flash to inactive slot.
      info('Extracting rootfs partition...');
      const rootfsPartition = `${loopDevice}p2`;

      // Use dd to extract just the rootfs partition to a file.
      execSync(`sudo dd if="${rootfsPartition}" of="${rootfsRaw}" bs=4M`, { stdio: 'pipe' });

      // Now mount the extracted rootfs to inject SSH key.
      const rootfsLoop = execSync(`sudo losetup -f --show "${rootfsRaw}"`, {
        encoding: 'utf-8',
        stdio: 'pipe',
      }).trim();

      try {
        execSync(`mkdir -p "${mountPoint}"`, { stdio: 'pipe' });
        info('Mounting extracted rootfs...');
        execSync(`sudo mount "${rootfsLoop}" "${mountPoint}"`, { stdio: 'pipe' });
        activeMountPoint = mountPoint;

        try {
          // Inject SSH key.
          if (config && config.ssh_key_path) {
            info(`Injecting SSH key: ${basename(config.ssh_key_path)}`);
            const sshKey = readFileSync(config.ssh_key_path, 'utf-8').trim();
            const authorizedKeysPath = join(mountPoint, `home/${username}/.ssh/authorized_keys`);

            execSync(`echo '${sshKey}' | sudo tee "${authorizedKeysPath}" > /dev/null`, { stdio: 'pipe' });
            execSync(`sudo chmod 600 "${authorizedKeysPath}"`, { stdio: 'pipe' });
            execSync(`sudo chown 1000:1000 "${authorizedKeysPath}"`, { stdio: 'pipe' });
            success('SSH key injected!');
          }

        } finally {
          // Unmount.
          info('Unmounting...');
          execSync(`sudo umount "${mountPoint}"`, { stdio: 'pipe' });
          activeMountPoint = null;
        }

        // Sync and detach rootfs loop device.
        execSync('sync', { stdio: 'pipe' });
        execSync(`sudo losetup -d "${rootfsLoop}"`, { stdio: 'pipe' });

      } catch (err) {
        // Cleanup rootfs loop on error.
        execSync(`sudo losetup -d "${rootfsLoop}" 2>/dev/null || true`, { stdio: 'pipe' });
        throw err;
      }

    } finally {
      // Detach main loop device.
      info('Detaching loop device...');
      execSync(`sudo losetup -d "${loopDevice}"`, { stdio: 'pipe' });
      activeLoopDevice = null;
    }

    // Compress the rootfs.
    info('Compressing rootfs...');
    execSync(`gzip -c "${rootfsRaw}" > "${preparedRootfsPath}"`, { stdio: 'pipe' });

    // Clean up.
    unlinkSync(wicPath);
    unlinkSync(rootfsRaw);
    rmSync(mountPoint, { recursive: true, force: true });

    success('Rootfs prepared!');
    return { preparedRootfsPath, workDir };

  } catch (err) {
    // Clean up on error.
    try {
      execSync(`sudo umount "${mountPoint}" 2>/dev/null || true`, { stdio: 'pipe' });
      execSync(`sudo losetup -D 2>/dev/null || true`, { stdio: 'pipe' });
      if (existsSync(wicPath)) unlinkSync(wicPath);
      if (existsSync(rootfsRaw)) unlinkSync(rootfsRaw);
      if (existsSync(mountPoint)) rmSync(mountPoint, { recursive: true, force: true });
      if (existsSync(workDir)) rmSync(workDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors.
    }
    throw err;
  }
}

/**
 * Clean up prepared rootfs temp directory.
 * @param {string} workDir - Temp directory to remove.
 */
export function cleanupPreparedImage(workDir) {
  try {
    execSync(`rm -rf "${workDir}"`, { stdio: 'pipe' });
  } catch {
    warn(`Failed to clean up temp directory: ${workDir}`);
  }
}

/**
 * Flash prepared image to remote device using ab-update.
 * This is the point of no return - runs remote A/B update and reboots.
 * @param {string} remoteImagePath - Path to image on remote system.
 * @param {string} device - Device to flash (e.g., /dev/sda).
 * @param {string} remoteTarget - user@host string.
 * @param {boolean} dryRun - If true, skip actual flash.
 * @param {boolean} skipConfirm - If true, skip confirmation prompt.
 * @returns {Promise<void>}
 */
export async function remoteFlash(remoteImagePath, device, remoteTarget, dryRun = false, skipConfirm = false) {
  // Extract hostname from remoteTarget for display.
  const remoteHost = remoteTarget.split('@')[1] || remoteTarget;

  console.log('');
  console.log(`${colors.yellow}    ☠️  YOLO MODE - NO SAFETY NET  ☠️${colors.reset}`);
  console.log(`${colors.dim}    If this fails, pull the disk and reflash.${colors.reset}`);
  console.log('');

  if (dryRun) {
    console.log(`${colors.yellow}DRY RUN - would execute:${colors.reset}`);
    console.log('');
    console.log(`  # A/B Update using ab-update helper`);
    console.log(`  ab-update ${remoteImagePath}`);
    console.log('');
    console.log(`  # Reboot to activate new slot`);
    console.log(`  sudo systemctl reboot`);
    console.log('');
    return;
  }

  // Final confirmation.
  console.log(`${colors.bold}${colors.red}═══════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}${colors.red}  THIS WILL OVERWRITE ${device} ON ${remoteHost}${colors.reset}`);
  console.log(`${colors.bold}${colors.red}  The system may become unresponsive during the write.${colors.reset}`);
  console.log(`${colors.bold}${colors.red}  If it fails, you'll need to pull the disk and reflash.${colors.reset}`);
  console.log(`${colors.bold}${colors.red}═══════════════════════════════════════════════════════════════${colors.reset}`);
  console.log('');

  if (!skipConfirm) {
    const confirm = await prompt(`Type "yolo" to proceed: `);
    if (confirm.toLowerCase() !== 'yolo') {
      error('Aborted.');
      process.exit(1);
    }
  } else {
    console.log(`${colors.yellow}🍺 Hold my mead... here we go!${colors.reset}`);
  }

  console.log('');
  info('Running A/B update on Pi...');
  console.log('');

  // Run ab-update which flashes to inactive partition and switches boot slot.
  // This is SAFE because we're writing to the inactive partition, not the running one.
  try {
    const updateCmd = `ab-update ${remoteImagePath}`;
    await sshRun(remoteTarget, updateCmd);

    success('A/B update complete!');
    console.log('');
    info('Rebooting to activate new rootfs...');

    // Reboot to new slot.
    runCapture(`ssh -o ConnectTimeout=5 -o BatchMode=yes ${remoteTarget} "sudo systemctl reboot"`);

  } catch (err) {
    error(`A/B update failed: ${err.message}`);
    throw err;
  }

  // Give it a moment to start rebooting.
  await new Promise(resolve => setTimeout(resolve, 2000));
}

/**
 * Flash image to remote device using ab-update-with-key.
 * This performs SSH key injection on the Pi, eliminating the need for local sudo.
 * @param {string} remoteImagePath - Path to rootfs.ext4.gz on remote system.
 * @param {string|null} remoteKeyPath - Path to SSH public key on remote system (or null to skip).
 * @param {string} username - Username for SSH key injection (e.g., 'dirtsim').
 * @param {string} remoteTarget - user@host string.
 * @param {boolean} dryRun - If true, skip actual flash.
 * @param {boolean} skipConfirm - If true, skip confirmation prompt.
 * @param {string} updateScript - Path or name of update script (default: 'ab-update-with-key').
 * @returns {Promise<void>}
 */
export async function remoteFlashWithKey(remoteImagePath, remoteKeyPath, username, remoteTarget, dryRun = false, skipConfirm = false, updateScript = 'ab-update-with-key') {
  // Extract hostname from remoteTarget for display.
  const remoteHost = remoteTarget.split('@')[1] || remoteTarget;

  console.log('');
  console.log(`${colors.yellow}    ☠️  YOLO MODE - A/B Update  ☠️${colors.reset}`);
  console.log(`${colors.dim}    Flashing to inactive slot. Previous slot remains intact.${colors.reset}`);
  console.log('');

  // Build the command with optional key path.
  let updateCmd = `${updateScript} ${remoteImagePath}`;
  if (remoteKeyPath) {
    updateCmd += ` ${remoteKeyPath} ${username}`;
  }

  if (dryRun) {
    console.log(`${colors.yellow}DRY RUN - would execute:${colors.reset}`);
    console.log('');
    console.log(`  # A/B Update with key injection`);
    console.log(`  ${updateCmd}`);
    console.log('');
    console.log(`  # Reboot to activate new slot`);
    console.log(`  sudo systemctl reboot`);
    console.log('');
    return;
  }

  // Final confirmation.
  console.log(`${colors.bold}${colors.cyan}═══════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}${colors.cyan}  Flashing to inactive partition on ${remoteHost}${colors.reset}`);
  if (remoteKeyPath) {
    console.log(`${colors.bold}${colors.cyan}  SSH key will be injected for user: ${username}${colors.reset}`);
  }
  console.log(`${colors.bold}${colors.cyan}  The previous slot remains bootable if this fails.${colors.reset}`);
  console.log(`${colors.bold}${colors.cyan}═══════════════════════════════════════════════════════════════${colors.reset}`);
  console.log('');

  if (!skipConfirm) {
    const confirm = await prompt(`Type "yolo" to proceed: `);
    if (confirm.toLowerCase() !== 'yolo') {
      error('Aborted.');
      process.exit(1);
    }
  } else {
    console.log(`${colors.yellow}🍺 Hold my mead... here we go!${colors.reset}`);
  }

  console.log('');
  info('Running A/B update with key injection on Pi...');
  console.log('');

  // Run ab-update-with-key which flashes and optionally injects SSH key.
  try {
    await sshRun(remoteTarget, updateCmd);

    success('A/B update complete!');
    console.log('');
    info('Rebooting to activate new rootfs...');

    // Reboot to new slot.
    runCapture(`ssh -o ConnectTimeout=5 -o BatchMode=yes ${remoteTarget} "sudo systemctl reboot"`);

  } catch (err) {
    error(`A/B update failed: ${err.message}`);
    throw err;
  }

  // Give it a moment to start rebooting.
  await new Promise(resolve => setTimeout(resolve, 2000));
}
