import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import type { Config } from '../config.js';
import { FlowError } from '../lib/errors.js';
import type { Logger } from '../lib/logger.js';

export function cdpUrl(port: number): string {
  return `http://127.0.0.1:${port}`;
}

export async function cdpReachable(port: number): Promise<boolean> {
  try {
    const res = await fetch(`${cdpUrl(port)}/json/version`, { signal: AbortSignal.timeout(1500) });
    return res.ok;
  } catch {
    return false;
  }
}

async function browserIdentity(config: Config): Promise<string> {
  const response = await fetch(`${cdpUrl(config.cdpPort)}/json/version`, { signal: AbortSignal.timeout(3000) });
  const data = (await response.json()) as { webSocketDebuggerUrl?: string };
  const url = new URL(data.webSocketDebuggerUrl ?? '');
  if (
    url.protocol !== 'ws:' ||
    !['localhost', '127.0.0.1', '[::1]'].includes(url.hostname) ||
    Number(url.port) !== config.cdpPort ||
    !url.pathname.startsWith('/devtools/browser/')
  ) {
    throw Error('Unexpected Chrome debugging endpoint.');
  }
  return url.pathname;
}

const ownership = (config: Config) => ({
  id: config.connectionId,
  directory:
    process.platform === 'win32'
      ? path.resolve(config.userDataDir).toLowerCase()
      : path.resolve(config.userDataDir),
  profile:
    process.platform === 'win32' ? config.profileDirectory.toLowerCase() : config.profileDirectory,
  port: config.cdpPort,
});

export async function verifyManagedChrome(config: Config): Promise<void> {
  if (!config.connectionId) return;
  await rememberManagedChrome(config).catch(() => undefined);
}

export async function rememberManagedChrome(config: Config): Promise<void> {
  if (!config.connectionId) return;
  await fs.promises.mkdir(config.stateDir, { recursive: true });
  await fs.promises.writeFile(
    path.join(config.stateDir, 'chrome-owner.json'),
    JSON.stringify({ ...ownership(config), browser: await browserIdentity(config) }),
  );
}

// Chrome is spawned outside Playwright so navigator.webdriver stays false: Google sign-in rejects automated browsers
export async function launchChrome(config: Config, log: Logger): Promise<void> {
  if (!fs.existsSync(config.chromePath)) {
    throw new FlowError('CHROME_LAUNCH_FAILED', `Chrome not found at ${config.chromePath}`, {
      chromePath: config.chromePath,
    });
  }
  fs.mkdirSync(config.userDataDir, { recursive: true });
  const args = [
    `--remote-debugging-port=${config.cdpPort}`,
    `--user-data-dir=${config.userDataDir}`,
    `--profile-directory=${config.profileDirectory}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-blink-features=AutomationControlled',
    '--disable-extensions',
    '--disable-background-timer-throttling',
    '--disable-backgrounding-occluded-windows',
    '--disable-renderer-backgrounding',
    '--window-position=40,40',
    '--window-size=1440,900',
  ];
  log.info('launching chrome', {
    chromePath: config.chromePath,
    cdpPort: config.cdpPort,
    userDataDir: config.userDataDir,
  });
  const child = spawn(config.chromePath, args, { detached: true, stdio: 'ignore' });
  child.on('error', (err) => {
    log.error('chrome process error', { error: err.message });
  });
  child.unref();

  for (let attempt = 0; attempt < 20; attempt++) {
    if (await cdpReachable(config.cdpPort)) return;
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new FlowError('CHROME_LAUNCH_FAILED', `Chrome did not open CDP port ${config.cdpPort} within 20s`, {
    cdpPort: config.cdpPort,
  });
}
