/**
 * CLI utilities for flash scripts.
 * Provides colors, logging, prompting, and formatting helpers.
 */

import { createInterface } from 'readline';

// Prefix for temporary directories created by flash utilities.
export const TEMP_PREFIX = 'pi-base-';

// Terminal colors.
export const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
};

// Logging functions.
export function log(msg) {
  console.log(msg);
}

export function info(msg) {
  console.log(`${colors.blue}ℹ${colors.reset} ${msg}`);
}

export function success(msg) {
  console.log(`${colors.green}✓${colors.reset} ${msg}`);
}

export function warn(msg) {
  console.log(`${colors.yellow}⚠${colors.reset} ${msg}`);
}

export function error(msg) {
  console.log(`${colors.red}✗${colors.reset} ${msg}`);
}

/**
 * Prompt user for input.
 * @param {string} question - The question to ask.
 * @returns {Promise<string>} The user's trimmed response.
 */
export async function prompt(question) {
  const rl = createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  return new Promise(resolve => {
    rl.question(question, answer => {
      rl.close();
      resolve(answer.trim());
    });
  });
}

/**
 * Format bytes to human readable string.
 * @param {number} bytes - Number of bytes.
 * @returns {string} Formatted string like "1.5 GB".
 */
export function formatBytes(bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (bytes >= 1024 && i < units.length - 1) {
    bytes /= 1024;
    i++;
  }
  return `${bytes.toFixed(1)} ${units[i]}`;
}

/**
 * Setup consola logger with timestamp reporter.
 * Creates a consola instance configured with detailed timestamps (HH:MM:SS.mmm).
 * Note: consola must be installed as a dependency.
 * @returns {object} Configured consola instance.
 */
export function setupConsolaLogging() {
  // Note: consola is imported by the caller who needs it.
  // This function is a factory for the timestamp reporter configuration.

  // Custom reporter with detailed timestamps (HH:MM:SS.mmm).
  const timestampReporter = {
    log(logObj) {
      const d = new Date(logObj.date);
      const hours = String(d.getHours()).padStart(2, '0');
      const minutes = String(d.getMinutes()).padStart(2, '0');
      const seconds = String(d.getSeconds()).padStart(2, '0');
      const ms = String(d.getMilliseconds()).padStart(3, '0');
      const timestamp = `${hours}:${minutes}:${seconds}.${ms}`;

      // Badge based on type.
      const badge = logObj.type === 'success' ? '✔' :
                    logObj.type === 'error' ? '✖' :
                    logObj.type === 'warn' ? '⚠' :
                    logObj.type === 'info' ? 'ℹ' :
                    logObj.type === 'start' ? '▶' : ' ';

      // Color based on type.
      const color = logObj.type === 'success' ? '\x1b[32m' :
                    logObj.type === 'error' ? '\x1b[31m' :
                    logObj.type === 'warn' ? '\x1b[33m' :
                    logObj.type === 'info' ? '\x1b[36m' : '';

      const reset = '\x1b[0m';
      const dim = '\x1b[2m';

      console.log(`${dim}[${timestamp}]${reset} ${color}${badge}${reset} ${logObj.args.join(' ')}`);
    },
  };

  return timestampReporter;
}

/**
 * Display a banner box.
 * @param {string} title - Title to display.
 * @param {object} consola - Optional consola instance (uses consola.box if available).
 */
export function banner(title, consola = null) {
  if (consola && consola.box) {
    consola.box(title);
  } else {
    // Fallback for non-consola usage.
    console.log('');
    console.log(`${colors.bold}${colors.cyan}${title}${colors.reset}`);
    console.log('');
  }
}

/**
 * Display YOLO warning banner.
 */
export function skull() {
  console.log('');
  console.log(`${colors.yellow}    ☠️  YOLO MODE - NO SAFETY NET  ☠️${colors.reset}`);
  console.log(`${colors.dim}    If this fails, pull the disk and reflash.${colors.reset}`);
  console.log('');
}
