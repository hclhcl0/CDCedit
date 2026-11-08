import type { Page } from 'playwright-core';
import { FlowError } from '../lib/errors.js';
import type { Logger } from '../lib/logger.js';
import { sleep } from './ui.js';

// Flow mints scene ids in upper case and project ids in lower case, so both are accepted
const UUID = '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}';
// Flow moved to flow.google.com/project/<id> (2026-09-05); the labs.google form still redirects there and stays
// accepted as input. Scene Builder lives under /scene/<id>; /scenes/ is accepted for older links. Anything else
// below a project (a media detail view, say) must not parse as "on the project", or navigation would be skipped.
export const FLOW_ROOT = 'https://flow.google.com';
export const FLOW_URL_RE = new RegExp(
  `^https://(?:flow\\.google\\.com|labs\\.google/fx/(?:[a-z]{2}(?:-[A-Za-z]{2})?/)?tools/flow)` +
    `(?:/project/(${UUID})(?:/scenes?/(${UUID}))?)?/?(?:[?#].*)?$`,
);

export interface FlowLocation {
  projectId: string | undefined;
  sceneId: string | undefined;
}

export function parseFlowUrl(url: string): FlowLocation | null {
  const m = FLOW_URL_RE.exec(url);
  if (!m) return null;
  return { projectId: m[1], sceneId: m[2] };
}

export function requireProjectUrl(url: string): string {
  const location = parseFlowUrl(url);
  if (!location?.projectId) {
    throw new FlowError('PROJECT_NOT_FOUND', `not a Flow project URL: ${url}`, { project_url: url });
  }
  return location.projectId;
}

export function sceneUrl(projectUrl: string, sceneId: string): string {
  return `${FLOW_ROOT}/project/${requireProjectUrl(projectUrl)}/scene/${sceneId}`;
}

export async function setChromeWindowState(page: Page, _windowState: 'minimized' | 'normal'): Promise<void> {
  try {
    const cdp = await page.context().newCDPSession(page);
    const { windowId, bounds } = (await cdp.send('Browser.getWindowForTarget')) as {
      windowId: number;
      bounds?: { windowState?: string };
    };
    if (bounds?.windowState === 'minimized') {
      await cdp.send('Browser.setWindowBounds', { windowId, bounds: { windowState: 'normal' } });
    }
    await cdp.detach();
  } catch {
    // Ignore if CDP target does not support window bounds
  }
}

// Navigates only when the tab is not already on that project (scene view counts as a different place)
export async function ensureOnProject(page: Page, projectUrl: string, log: Logger, settleMs = 5000): Promise<string> {
  const projectId = requireProjectUrl(projectUrl);
  const here = parseFlowUrl(page.url());
  if (here?.projectId === projectId && !here.sceneId && isLoggedIn(page)) {
    await setChromeWindowState(page, 'minimized');
    return projectId;
  }
  log.info('navigating to project', { projectUrl });
  await page.goto(projectUrl, { waitUntil: 'domcontentloaded' });
  await sleep(settleMs);
  if (!isLoggedIn(page)) {
    await setChromeWindowState(page, 'normal');
    await page.bringToFront().catch(() => {});
  }
  assertLoggedIn(page);
  const landed = parseFlowUrl(page.url());
  if (landed?.projectId !== projectId) {
    throw new FlowError(
      'PROJECT_NOT_FOUND',
      `Could not open Flow project (landed on ${page.url()}). Check that you are signed into the right Google account in Chrome.`,
      { project_url: projectUrl, current_url: page.url() },
    );
  }
  await setChromeWindowState(page, 'minimized');
  return projectId;
}

export async function ensureOnScene(page: Page, url: string, log: Logger, settleMs = 4000): Promise<string> {
  const loc = parseFlowUrl(url);
  if (!loc?.sceneId) throw new FlowError('SCENE_NOT_FOUND', `not a Flow scene URL: ${url}`, { scene_url: url });
  const here = parseFlowUrl(page.url());
  if (here?.sceneId === loc.sceneId && isLoggedIn(page)) {
    await setChromeWindowState(page, 'minimized');
    return loc.sceneId;
  }
  log.info('navigating to scene', { url });
  await page.goto(url, { waitUntil: 'domcontentloaded' });
  await sleep(settleMs);
  if (!isLoggedIn(page)) {
    await setChromeWindowState(page, 'normal');
    await page.bringToFront().catch(() => {});
  }
  assertLoggedIn(page);
  await setChromeWindowState(page, 'minimized');
  return loc.sceneId;
}

export function isLoggedIn(page: Page): boolean {
  const url = page.url();
  if (!url || url === 'about:blank' || url.startsWith('chrome://')) return false;
  if (url.includes('accounts.google.com')) return false;
  // Unauthenticated visits to flow.google.com or /project/<id> redirect to /about marketing page
  if (/^https:\/\/flow\.google\.com\/about(?:[/?#]|$)/.test(url)) return false;
  return parseFlowUrl(url) !== null;
}

export function assertLoggedIn(page: Page): void {
  if (!isLoggedIn(page)) {
    throw new FlowError(
      'NOT_LOGGED_IN',
      'Chrome has not signed into Google Flow yet; sign in once in the Flow Chrome window and try again.',
      {
        url: page.url(),
      },
    );
  }
}

export async function openFlowHome(page: Page, flowUrl: string, settleMs = 4000): Promise<void> {
  await page.goto(flowUrl, { waitUntil: 'domcontentloaded' });
  await sleep(settleMs);
  if (isLoggedIn(page)) {
    await setChromeWindowState(page, 'minimized');
  } else {
    await setChromeWindowState(page, 'normal');
    await page.bringToFront().catch(() => {});
  }
}

export async function createNewProject(
  page: Page,
  log: Logger,
  settleMs = 5000,
): Promise<{ project_id: string; project_url: string }> {
  log.info('navigating to Flow root to create new project');
  await page.goto(FLOW_ROOT, { waitUntil: 'domcontentloaded' });
  await sleep(settleMs);
  if (!isLoggedIn(page)) {
    await setChromeWindowState(page, 'normal');
    await page.bringToFront().catch(() => {});
  }
  assertLoggedIn(page);

  const newProjectBtn = page
    .locator('button:has-text("New project"), [role="button"]:has-text("New project"), button[aria-label*="New project"]')
    .first();

  if ((await newProjectBtn.count()) === 0) {
    throw new FlowError(
      'UI_NOT_FOUND',
      'Không tìm thấy nút "New project" trên trang chủ Flow. Hãy kiểm tra giao diện Flow trong Chrome.',
      { url: page.url() },
    );
  }

  log.info('clicking "New project" button');
  await Promise.all([
    page.waitForURL(FLOW_URL_RE, { timeout: 15000 }).catch(() => {}),
    newProjectBtn.click(),
  ]);

  await sleep(3000);
  const landed = parseFlowUrl(page.url());
  if (!landed?.projectId) {
    throw new FlowError(
      'PROJECT_NOT_FOUND',
      `Không thể tạo dự án Flow mới (URL hiện tại: ${page.url()}). Hãy thử lại hoặc tạo thủ công trên Flow.`,
      { current_url: page.url() },
    );
  }

  await setChromeWindowState(page, 'minimized');
  const projectUrl = `${FLOW_ROOT}/project/${landed.projectId}`;
  log.info('successfully created new Flow project', { projectId: landed.projectId, projectUrl });
  return {
    project_id: landed.projectId,
    project_url: projectUrl,
  };
}
