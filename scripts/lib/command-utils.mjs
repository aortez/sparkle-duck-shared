/**
 * Command execution utilities for flash scripts.
 * Provides wrappers for spawning processes and capturing output.
 */

import { execSync, spawn } from 'child_process';

/**
 * Run a command with inherited stdio (live output to terminal).
 * @param {string} cmd - Command to execute.
 * @param {string[]} args - Command arguments.
 * @param {object} options - spawn options (cwd, env, etc.).
 * @returns {Promise<void>} Resolves on success, rejects with error.
 */
export async function run(cmd, args, options = {}) {
  return new Promise((resolve, reject) => {
    const proc = spawn(cmd, args, { stdio: 'inherit', ...options });
    proc.on('close', code => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${cmd} exited with code ${code}`));
      }
    });
    proc.on('error', reject);
  });
}

/**
 * Run a command and capture output.
 * @param {string} cmd - Command string to execute.
 * @param {object} options - execSync options.
 * @returns {string|null} Trimmed stdout or null on error.
 */
export function runCapture(cmd, options = {}) {
  try {
    return execSync(cmd, { encoding: 'utf-8', stdio: 'pipe', ...options }).trim();
  } catch (err) {
    return null;
  }
}
