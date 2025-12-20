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
