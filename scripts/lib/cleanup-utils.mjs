/**
 * Resource cleanup and signal handling utilities.
 * Provides tracking for temporary resources and emergency cleanup on interruption.
 */

import { execSync } from 'child_process';
import { colors } from './cli-utils.mjs';

/**
 * Create a cleanup manager for tracking resources and handling signals.
 * Provides signal handling and emergency cleanup for Ctrl+C interruption.
 * @returns {object} Cleanup manager with resource tracking and signal handling methods.
 */
export function createCleanupManager() {
  // Track resources for cleanup.
  const resources = {
    loop: null,
    mount: null,
    tempdir: null,
  };

  let inCriticalSection = false;
  let sigintHandler = null;

  /**
   * Emergency cleanup function called on SIGINT.
   */
  function emergencyCleanup() {
    // If we're in the critical section (dd running), refuse to exit.
    if (inCriticalSection) {
      console.log('');
      console.log(`${colors.bold}${colors.red}═══════════════════════════════════════════════════════════════${colors.reset}`);
      console.log(`${colors.bold}${colors.red}  ⚠️  CANNOT INTERRUPT - CRITICAL OPERATION IN PROGRESS!${colors.reset}`);
      console.log(`${colors.bold}${colors.red}  Ctrl+C disabled to prevent corruption.${colors.reset}`);
      console.log(`${colors.bold}${colors.red}  Wait for operation to complete...${colors.reset}`);
      console.log(`${colors.bold}${colors.red}═══════════════════════════════════════════════════════════════${colors.reset}`);
      console.log('');
      return; // Don't exit.
    }

    console.log('');
    console.log(`${colors.yellow}⚠${colors.reset} Ctrl+C detected - cleaning up...`);

    try {
      if (resources.mount) {
        console.log(`${colors.blue}ℹ${colors.reset} Unmounting ${resources.mount}...`);
        execSync(`sudo umount ${resources.mount} 2>/dev/null || true`, { stdio: 'pipe' });
      }
      if (resources.loop) {
        console.log(`${colors.blue}ℹ${colors.reset} Detaching ${resources.loop}...`);
        execSync(`sudo losetup -d ${resources.loop} 2>/dev/null || true`, { stdio: 'pipe' });
      }
      if (resources.tempdir) {
        console.log(`${colors.blue}ℹ${colors.reset} Removing temp directory...`);
        execSync(`rm -rf ${resources.tempdir}`, { stdio: 'pipe' });
      }
      console.log(`${colors.green}✓${colors.reset} Cleanup complete.`);
    } catch (err) {
      console.log(`${colors.red}✗${colors.reset} Cleanup failed: ${err.message}`);
    }

    console.log('');
    process.exit(130); // Standard exit code for SIGINT.
  }

  return {
    /**
     * Track a resource for cleanup.
     * @param {string} type - Resource type: 'loop', 'mount', or 'tempdir'.
     * @param {string} value - Resource path/device.
     */
    trackResource(type, value) {
      if (!['loop', 'mount', 'tempdir'].includes(type)) {
        throw new Error(`Invalid resource type: ${type}`);
      }
      resources[type] = value;
    },

    /**
     * Remove a tracked resource.
     * @param {string} type - Resource type to remove.
     */
    untrackResource(type) {
      if (!['loop', 'mount', 'tempdir'].includes(type)) {
        throw new Error(`Invalid resource type: ${type}`);
      }
      resources[type] = null;
    },

    /**
     * Enter critical section (disable cleanup on interrupt).
     */
    enterCriticalSection() {
      inCriticalSection = true;
    },

    /**
     * Exit critical section (re-enable cleanup).
     */
    exitCriticalSection() {
      inCriticalSection = false;
    },

    /**
     * Check if in critical section.
     * @returns {boolean} True if in critical section.
     */
    isInCriticalSection() {
      return inCriticalSection;
    },

    /**
     * Manual cleanup (called on SIGINT or programmatically).
     * Does not exit process by default - caller decides.
     * @param {number} exitCode - Exit code (default: 130 for SIGINT).
     */
    cleanup(exitCode = 130) {
      if (inCriticalSection) {
        console.warn('Cannot cleanup during critical section.');
        return;
      }

      try {
        if (resources.mount) {
          execSync(`sudo umount ${resources.mount} 2>/dev/null || true`, { stdio: 'pipe' });
        }
        if (resources.loop) {
          execSync(`sudo losetup -d ${resources.loop} 2>/dev/null || true`, { stdio: 'pipe' });
        }
        if (resources.tempdir) {
          execSync(`rm -rf ${resources.tempdir}`, { stdio: 'pipe' });
        }
      } catch (err) {
        console.error(`Cleanup error: ${err.message}`);
      }
    },

    /**
     * Install SIGINT handler.
     */
    installSignalHandlers() {
      if (sigintHandler) {
        throw new Error('Signal handlers already installed.');
      }
      sigintHandler = emergencyCleanup;
      process.on('SIGINT', sigintHandler);
    },

    /**
     * Uninstall SIGINT handler.
     */
    uninstallSignalHandlers() {
      if (sigintHandler) {
        process.off('SIGINT', sigintHandler);
        sigintHandler = null;
      }
    },
  };
}
