/**
 * Configuration utilities for flash scripts.
 * Manages .flash-config.json for storing user preferences.
 */

import { existsSync, readFileSync, writeFileSync } from 'fs';
import { warn } from './cli-utils.mjs';

/**
 * Load flash configuration from a JSON file.
 * @param {string} configPath - Path to the config file.
 * @returns {object|null} The config object, or null if invalid/missing.
 *
 * Supported fields:
 *   - ssh_key_path (string, required): Path to SSH public key.
 *   - device (string, optional): Device path (e.g., "/dev/sdb").
 *   - hostname (string, optional): Target device hostname.
 *   - backup_data (boolean, optional): Auto-backup data partition.
 *   - skip_confirmation (boolean, optional): Skip final flash confirmation.
 */
export function loadConfig(configPath) {
  try {
    if (!existsSync(configPath)) {
      return null;
    }
    const content = readFileSync(configPath, 'utf-8');
    const config = JSON.parse(content);
    // Validate required fields.
    if (!config.ssh_key_path || typeof config.ssh_key_path !== 'string') {
      return null;
    }
    // Check that the key file still exists.
    if (!existsSync(config.ssh_key_path)) {
      warn(`Configured SSH key no longer exists: ${config.ssh_key_path}`);
      return null;
    }
    return config;
  } catch {
    return null;
  }
}

/**
 * Save flash configuration to a JSON file.
 * @param {string} configPath - Path to the config file.
 * @param {object} config - The config object to save.
 * @returns {boolean} True if saved successfully.
 */
export function saveConfig(configPath, config) {
  try {
    writeFileSync(configPath, JSON.stringify(config, null, 2) + '\n');
    return true;
  } catch (err) {
    warn(`Failed to save config: ${err.message}`);
    return false;
  }
}
