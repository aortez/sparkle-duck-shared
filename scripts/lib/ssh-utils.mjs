/**
 * SSH utilities for flash scripts.
 * Handles SSH key discovery, reading, and injection into images.
 */

import { execSync } from 'child_process';
import { existsSync, readdirSync, readFileSync, mkdtempSync, rmdirSync } from 'fs';
import { join, basename } from 'path';
import { homedir, tmpdir } from 'os';
import { colors, log, info, success, warn, error, prompt, TEMP_PREFIX } from './cli-utils.mjs';
import { saveConfig } from './config-utils.mjs';
import { getPartitionDevice } from './partition-utils.mjs';

/**
 * Find available SSH public keys in ~/.ssh/.
 * @returns {Array<{name: string, path: string}>} Array of key info objects.
 */
export function findSshKeys() {
  const sshDir = join(homedir(), '.ssh');
  if (!existsSync(sshDir)) {
    return [];
  }

  try {
    return readdirSync(sshDir)
      .filter(f => f.endsWith('.pub'))
      .map(f => ({
        name: f,
        path: join(sshDir, f),
      }))
      .filter(k => existsSync(k.path));
  } catch {
    return [];
  }
}

/**
 * Read the contents of an SSH public key file.
 * @param {string} keyPath - Path to the public key file.
 * @returns {string|null} The key contents, or null on error.
 */
export function readSshKey(keyPath) {
  try {
    return readFileSync(keyPath, 'utf-8').trim();
  } catch (err) {
    error(`Failed to read SSH key: ${err.message}`);
    return null;
  }
}

/**
 * Interactively select an SSH key and save to config.
 * @param {string} configPath - Path to save the config.
 * @returns {Promise<object>} The config object with ssh_key_path.
 */
export async function configureSSHKey(configPath) {
  log('');
  log(`${colors.bold}${colors.cyan}SSH Key Configuration${colors.reset}`);
  log('');
  info('The image uses SSH key authentication (no passwords).');
  info('Select which public key to install on the device.');
  log('');

  const keys = findSshKeys();

  if (keys.length === 0) {
    error('No SSH public keys found in ~/.ssh/');
    error('Generate one with: ssh-keygen -t ed25519');
    process.exit(1);
  }

  log(`${colors.bold}Available SSH keys:${colors.reset}`);
  log('');
  keys.forEach((key, i) => {
    log(`  ${colors.cyan}${i + 1})${colors.reset} ${key.name}`);
  });
  log('');

  const choice = await prompt(`Select key (1-${keys.length}): `);
  const index = parseInt(choice, 10) - 1;

  if (isNaN(index) || index < 0 || index >= keys.length) {
    error('Invalid selection.');
    process.exit(1);
  }

  const selectedKey = keys[index];
  const config = { ssh_key_path: selectedKey.path };

  if (saveConfig(configPath, config)) {
    success(`SSH key configured: ${selectedKey.name}`);
    info(`Config saved to: ${basename(configPath)}`);
  }

  return config;
}

/**
 * Inject SSH key into a mounted rootfs.
 * @param {string} device - The device (e.g., /dev/sdb).
 * @param {string} sshKeyPath - Path to the SSH public key.
 * @param {string} username - The username on the device.
 * @param {number} uid - The user's UID (default 1000).
 * @param {boolean} dryRun - If true, only show what would happen.
 */
export async function injectSSHKey(device, sshKeyPath, username, uid = 1000, dryRun = false) {
  const rootfsPartition = getPartitionDevice(device, 2);
  const sshKey = readSshKey(sshKeyPath);

  if (!sshKey) {
    throw new Error('Failed to read SSH key');
  }

  log('');
  info('Injecting SSH key into image...');

  if (dryRun) {
    log(`  Would mount ${rootfsPartition}`);
    log(`  Would write key to /home/${username}/.ssh/authorized_keys`);
    log(`  Would unmount`);
    return;
  }

  // Create temporary mount point.
  const mountPoint = mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}rootfs-`));

  try {
    // Mount the rootfs partition.
    info(`Mounting ${rootfsPartition}...`);
    execSync(`sudo mount ${rootfsPartition} ${mountPoint}`, { stdio: 'pipe' });

    // Write the SSH key.
    const authorizedKeysPath = join(mountPoint, `home/${username}/.ssh/authorized_keys`);
    info(`Writing SSH key to authorized_keys...`);
    execSync(`echo '${sshKey}' | sudo tee ${authorizedKeysPath} > /dev/null`, { stdio: 'pipe' });
    execSync(`sudo chmod 600 ${authorizedKeysPath}`, { stdio: 'pipe' });
    execSync(`sudo chown ${uid}:${uid} ${authorizedKeysPath}`, { stdio: 'pipe' });

    success('SSH key injected!');

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
