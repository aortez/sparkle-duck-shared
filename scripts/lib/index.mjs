/**
 * Pi Base Flash Script Utilities
 *
 * Shared utilities for flash scripts in Sparkle Duck family projects.
 *
 * Usage in your project's flash.mjs:
 *
 *   import {
 *     colors, log, info, success, warn, error, prompt, formatBytes,
 *     loadConfig, saveConfig,
 *     findSshKeys, readSshKey, configureSSHKey, injectSSHKey,
 *     hasDataPartition, backupDataPartition, restoreDataPartition, cleanupBackup, setHostname,
 *     getBlockDevices, findLatestImage, hasBmaptool, flashImage, displayDevices, selectDevice,
 *   } from 'sparkle-duck-shared/scripts/lib/index.mjs';
 *
 * Usage in your project's yolo-update.mjs:
 *
 *   import {
 *     setupConsolaLogging, banner, skull,
 *     run, runCapture,
 *     ssh, sshRun, checkRemoteReachable, transferImage, waitForReboot,
 *     getRemoteTmpSpace, getRemoteBootDevice, getRemoteUptime, getRemoteBootTime, verifyRemoteChecksum,
 *     calculateChecksum, prepareRootfs, remoteFlash, cleanupPreparedImage,
 *     createCleanupManager,
 *     loadConfig,
 *   } from '../path/to/pi-base/scripts/lib/index.mjs';
 */

// CLI utilities.
export {
  colors,
  log,
  info,
  success,
  warn,
  error,
  prompt,
  formatBytes,
  TEMP_PREFIX,
  setupConsolaLogging,
  banner,
  skull,
} from './cli-utils.mjs';

// Config utilities.
export {
  loadConfig,
  saveConfig,
} from './config-utils.mjs';

// SSH utilities.
export {
  findSshKeys,
  readSshKey,
  configureSSHKey,
  injectSSHKey,
} from './ssh-utils.mjs';

// Partition utilities.
export {
  hasDataPartition,
  backupDataPartition,
  restoreDataPartition,
  cleanupBackup,
  setHostname,
} from './partition-utils.mjs';

// Flash utilities.
export {
  getBlockDevices,
  findLatestImage,
  hasBmaptool,
  flashImage,
  displayDevices,
  selectDevice,
  validateDeviceIdentity,
  isLargeDevice,
} from './flash-utils.mjs';

// WiFi utilities.
export {
  generateWifiConnection,
  loadWifiCredsFile,
  getWifiCredentials,
  injectWifiCredentials,
} from './wifi-utils.mjs';

// Command utilities.
export {
  run,
  runCapture,
} from './command-utils.mjs';

// Cleanup utilities.
export {
  createCleanupManager,
} from './cleanup-utils.mjs';

// Remote utilities.
export {
  ssh,
  sshRun,
  checkRemoteReachable,
  getRemoteTmpSpace,
  getRemoteBootDevice,
  getRemoteUptime,
  getRemoteBootTime,
  waitForReboot,
  transferImage,
  verifyRemoteChecksum,
} from './remote-utils.mjs';

// Rootfs utilities.
export {
  calculateChecksum,
  prepareRootfs,
  cleanupPreparedImage,
  remoteFlash,
} from './rootfs-utils.mjs';
