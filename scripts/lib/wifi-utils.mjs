/**
 * WiFi utilities for flash scripts.
 * Handles WiFi credential generation, loading, and injection into images.
 */

import { execSync } from 'child_process';
import { existsSync, readFileSync, writeFileSync, mkdtempSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { colors, log, info, success, warn, prompt, TEMP_PREFIX } from './cli-utils.mjs';
import { getPartitionDevice } from './partition-utils.mjs';

/**
 * Generate a NetworkManager connection file for WiFi.
 * @param {string} ssid - The WiFi network name.
 * @param {string} password - The WiFi password.
 * @returns {string} The connection file content.
 */
export function generateWifiConnection(ssid, password) {
  // Generate a UUID for the connection.
  const uuid = 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });

  return `[connection]
id=${ssid}
uuid=${uuid}
type=wifi
autoconnect=true

[wifi]
mode=infrastructure
ssid=${ssid}

[wifi-security]
key-mgmt=wpa-psk
psk=${password}

[ipv4]
method=auto

[ipv6]
method=auto
`;
}

/**
 * Load WiFi credentials from a JSON file.
 * Expected format: { "ssid": "MyNetwork", "password": "secret" }
 * @param {string} credsFilePath - Path to the credentials file.
 * @returns {{ssid: string, password: string}|null} The credentials, or null if invalid.
 */
export function loadWifiCredsFile(credsFilePath) {
  try {
    if (!existsSync(credsFilePath)) {
      return null;
    }
    const content = readFileSync(credsFilePath, 'utf-8');
    const creds = JSON.parse(content);

    if (!creds.ssid || typeof creds.ssid !== 'string') {
      warn(`${credsFilePath}: missing or invalid "ssid" field`);
      return null;
    }
    if (!creds.password || typeof creds.password !== 'string') {
      warn(`${credsFilePath}: missing or invalid "password" field`);
      return null;
    }

    return { ssid: creds.ssid, password: creds.password };
  } catch (err) {
    if (err instanceof SyntaxError) {
      warn(`${credsFilePath}: invalid JSON - ${err.message}`);
    } else {
      warn(`${credsFilePath}: ${err.message}`);
    }
    return null;
  }
}

/**
 * Get WiFi credentials from file or interactive prompt.
 * @param {string} credsFilePath - Path to try loading credentials from.
 * @returns {Promise<{ssid: string, password: string}|null>} The credentials, or null if skipped.
 */
export async function getWifiCredentials(credsFilePath) {
  // First, try to load from file.
  const fileCreds = loadWifiCredsFile(credsFilePath);
  if (fileCreds) {
    log('');
    log(`${colors.bold}${colors.cyan}WiFi Configuration${colors.reset}`);
    log('');
    success(`Loaded credentials from ${credsFilePath}`);
    info(`Network: ${fileCreds.ssid}`);
    return fileCreds;
  }

  // Otherwise, prompt interactively.
  log('');
  log(`${colors.bold}${colors.cyan}WiFi Configuration${colors.reset}`);
  log('');
  info('Configure WiFi now so the device can connect on first boot.');
  info('Press Enter to skip (you can configure later with nmtui).');
  info(`Tip: Create ${credsFilePath} to avoid typing credentials.`);
  log('');

  const ssid = await prompt('WiFi network name (SSID): ');
  if (!ssid || !ssid.trim()) {
    info('Skipping WiFi configuration.');
    return null;
  }

  const password = await prompt('WiFi password: ');
  if (!password) {
    warn('No password provided - skipping WiFi configuration.');
    return null;
  }

  return { ssid: ssid.trim(), password };
}

/**
 * Inject WiFi credentials into the data partition.
 * Creates a NetworkManager connection file in /data/NetworkManager/system-connections/.
 * @param {string} device - The device path (e.g., /dev/sdb).
 * @param {string} ssid - The WiFi network name.
 * @param {string} password - The WiFi password.
 * @param {boolean} dryRun - If true, only show what would happen.
 */
export async function injectWifiCredentials(device, ssid, password, dryRun = false) {
  const dataPartition = getPartitionDevice(device, 4);
  const connectionContent = generateWifiConnection(ssid, password);
  // NetworkManager connection files use the SSID as filename.
  const filename = `${ssid}.nmconnection`;

  log('');
  info('Injecting WiFi credentials...');

  if (dryRun) {
    log(`  Would mount ${dataPartition}`);
    log(`  Would write ${filename} to /data/NetworkManager/system-connections/`);
    log(`  Would unmount`);
    return;
  }

  const mountPoint = mkdtempSync(join(tmpdir(), `${TEMP_PREFIX}data-wifi-`));

  try {
    // Mount the data partition.
    info(`Mounting ${dataPartition}...`);
    execSync(`sudo mount ${dataPartition} ${mountPoint}`, { stdio: 'pipe' });

    // Create NetworkManager directories.
    const nmDir = join(mountPoint, 'NetworkManager/system-connections');
    execSync(`sudo mkdir -p ${nmDir}`, { stdio: 'pipe' });
    execSync(`sudo chmod 755 ${join(mountPoint, 'NetworkManager')}`, { stdio: 'pipe' });
    execSync(`sudo chmod 700 ${nmDir}`, { stdio: 'pipe' });

    // Write the connection file.
    const connPath = join(nmDir, filename);
    info(`Writing ${filename}...`);
    // Use a temp file to avoid shell escaping issues with the password.
    const tempFile = join(tmpdir(), `nm-conn-${Date.now()}`);
    writeFileSync(tempFile, connectionContent, { mode: 0o600 });
    execSync(`sudo cp ${tempFile} ${connPath}`, { stdio: 'pipe' });
    execSync(`sudo chmod 600 ${connPath}`, { stdio: 'pipe' });
    rmSync(tempFile, { force: true });

    success(`WiFi "${ssid}" configured!`);

  } finally {
    // Always try to unmount and clean up.
    try {
      info('Unmounting...');
      execSync(`sudo umount ${mountPoint}`, { stdio: 'pipe' });
      rmSync(mountPoint, { recursive: true, force: true });
    } catch (err) {
      warn(`Cleanup warning: ${err.message}`);
    }
  }
}
