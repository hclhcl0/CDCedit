import { chromium, type Browser, type BrowserContext, type Page } from 'playwright-core';
import type { Config } from '../config.js';
import { parseFlowUrl } from '../flow/project.js';
import { FlowError } from '../lib/errors.js';
import type { Logger } from '../lib/logger.js';
import { RawCdp, findHungPages } from './cdp.js';
import { cdpReachable, cdpUrl, launchChrome, verifyManagedChrome, rememberManagedChrome } from './chrome.js';

export interface SessionStatus {
  connected: boolean;
  attached_existing: boolean;
  url: string | undefined;
}

async function pruneStuckTargets(
  endpoint: string,
  port: number,
  log?: Logger,
  options: { checkHung?: boolean; forceResetTabs?: boolean } = {},
): Promise<void> {
  try {
    const res = await fetch(`${endpoint}/json/list`, { signal: AbortSignal.timeout(3000) });
    if (!res.ok) return;
    const targets = (await res.json()) as Array<{ id?: string; type?: string; url?: string }>;
    const pages = targets.filter((t) => t.type === 'page');
    const hasUsableTab = pages.some((t) => {
      const u = t.url ?? '';
      return u !== '' && (!u.startsWith('chrome://') || u.startsWith('chrome://newtab'));
    });
    if (!hasUsableTab) {
      await fetch(`${endpoint}/json/new?about:blank`, {
        method: 'PUT',
        signal: AbortSignal.timeout(3000),
      }).catch(() => undefined);
    }
    for (const t of pages) {
      const u = t.url ?? '';
      if (t.id && (u === '' || (u.startsWith('chrome://') && !u.startsWith('chrome://newtab')))) {
        await fetch(`${endpoint}/json/close/${t.id}`, {
          signal: AbortSignal.timeout(2000),
        }).catch(() => undefined);
      }
    }
    if (options.forceResetTabs) {
      log?.warn('replacing wedged chrome tabs with fresh about:blank before retry');
      await fetch(`${endpoint}/json/new?about:blank`, {
        method: 'PUT',
        signal: AbortSignal.timeout(3000),
      }).catch(() => undefined);
      for (const t of pages) {
        if (t.id) {
          await fetch(`${endpoint}/json/close/${t.id}`, {
            signal: AbortSignal.timeout(2000),
          }).catch(() => undefined);
        }
      }
      return;
    }
    if (options.checkHung) {
      const hung = await findHungPages(port).catch(() => []);
      if (hung.length > 0) {
        log?.warn('closing hung chrome tabs before CDP attach', { count: hung.length, urls: hung.map((h) => h.url) });
        await fetch(`${endpoint}/json/new?about:blank`, {
          method: 'PUT',
          signal: AbortSignal.timeout(3000),
        }).catch(() => undefined);
        const cdp = await RawCdp.connect(port);
        try {
          for (const h of hung) await cdp.closeTarget(h.targetId);
        } finally {
          cdp.close();
        }
      }
    }
  } catch {
    // Ignore target pruning errors
  }
}

function pickExistingPage(context: BrowserContext): Page | undefined {
  const pages = context.pages().filter((p) => !p.isClosed());
  if (pages.length === 0) return undefined;
  // 1. Prefer a tab already on a Flow project
  const projectTab = pages.find((p) => parseFlowUrl(p.url())?.projectId !== undefined);
  if (projectTab) return projectTab;
  // 2. Prefer any tab on Flow or Google sign-in
  const flowTab = pages.find((p) => {
    const u = p.url();
    return (
      u.startsWith('https://flow.google.com') ||
      u.startsWith('https://labs.google') ||
      u.includes('accounts.google.com')
    );
  });
  if (flowTab) return flowTab;
  // 3. Reuse a blank / new-tab page so we don't leave extra tabs behind
  const blankTab = pages.find((p) => {
    const u = p.url();
    return u === '' || u === 'about:blank' || u.startsWith('chrome://newtab');
  });
  return blankTab ?? pages[0];
}

export class BrowserSession {
  private browser: Browser | undefined;
  private context: BrowserContext | undefined;
  private page: Page | undefined;
  private attachedExisting = false;

  constructor(
    private readonly config: Config,
    private readonly log: Logger,
  ) {}

  isConnected(): boolean {
    if (this.browser?.isConnected() !== true) return false;
    if (this.page !== undefined && !this.page.isClosed()) return true;
    if (this.context) {
      const fallback = pickExistingPage(this.context);
      if (fallback) {
        this.page = fallback;
        return true;
      }
    }
    return false;
  }

  status(): SessionStatus {
    const connected = this.isConnected();
    return {
      connected,
      attached_existing: this.attachedExisting,
      url: connected ? this.page?.url() : undefined,
    };
  }

  async ensureConnected(): Promise<Page> {
    const endpoint = cdpUrl(this.config.cdpPort);
    if (this.isConnected() && this.page) {
      await pruneStuckTargets(endpoint, this.config.cdpPort, this.log);
      const alive = await Promise.race([
        this.page.evaluate(() => true).catch(() => false),
        new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 2500)),
      ]);
      if (alive) return this.page;
    }
    this.reset();
    this.attachedExisting = await cdpReachable(this.config.cdpPort);
    if (this.config.attachOnly && !this.attachedExisting) {
      throw new FlowError(
        'BROWSER_NOT_CONNECTED',
        'The selected Chrome CDP port is not open. Start that Chrome session first, or choose a managed profile in Settings.',
      );
    }
    if (this.attachedExisting && !this.config.attachOnly) await verifyManagedChrome(this.config);
    if (!this.attachedExisting) {
      await launchChrome(this.config, this.log);
      await rememberManagedChrome(this.config);
    }
    await pruneStuckTargets(endpoint, this.config.cdpPort, this.log, { checkHung: true });
    try {
      this.browser = await chromium.connectOverCDP(endpoint, { timeout: 15_000 });
    } catch (firstErr) {
      this.log.warn('first CDP connect attempt failed; resetting wedged tabs and retrying', {
        error: errMessage(firstErr),
      });
      await pruneStuckTargets(endpoint, this.config.cdpPort, this.log, {
        checkHung: true,
        forceResetTabs: true,
      });
      try {
        this.browser = await chromium.connectOverCDP(endpoint, { timeout: 15_000 });
      } catch (err) {
        throw new FlowError('BROWSER_NOT_CONNECTED', `CDP connect failed: ${errMessage(err)}`, { endpoint });
      }
    }
    this.browser.on('disconnected', () => {
      this.log.warn('browser disconnected');
      this.reset();
    });
    this.context = this.browser.contexts()[0] ?? (await this.browser.newContext());
    this.page = pickExistingPage(this.context) ?? (await this.context.newPage());
    this.log.info('browser connected', {
      endpoint,
      attached_existing: this.attachedExisting,
      url: this.page.url(),
    });
    return this.page;
  }

  getPage(): Page {
    if (!this.isConnected() || !this.page) {
      throw new FlowError('BROWSER_NOT_CONNECTED', 'Browser not connected; call flow_connect first');
    }
    return this.page;
  }

  getContext(): BrowserContext {
    if (!this.isConnected() || !this.context) {
      throw new FlowError('BROWSER_NOT_CONNECTED', 'Browser not connected; call flow_connect first');
    }
    return this.context;
  }

  // Detach over CDP without closing the user's Flow tab or terminating Chrome.
  async close(): Promise<void> {
    const { browser } = this;
    this.reset();
    try {
      if (browser?.isConnected()) await browser.close();
    } catch (err) {
      this.log.warn('browser detach failed', { error: errMessage(err) });
    }
  }

  private reset(): void {
    this.browser = undefined;
    this.context = undefined;
    this.page = undefined;
  }
}

function errMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
