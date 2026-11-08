import type { Page } from 'playwright-core';
import { FlowError } from '../lib/errors.js';

async function expandAgentPanelIfNeeded(page: Page): Promise<void> {
  const expand = page.locator('button').filter({ hasText: /expand_content|open_in_full/ }).first();
  if (await expand.isVisible().catch(() => false)) {
    await expand.click({ timeout: 5_000 }).catch(() => undefined);
    await page.waitForTimeout(900);
  }
}

/** Enter the Agent composer without ever touching the generation/submit button. */
export async function ensureAgentMode(page: Page): Promise<void> {
  const agent = page.getByRole('button', { name: /^(Agent|에이전트)$/ }).last();
  const instructions = page.getByRole('button', { name: /^(Agent instructions|에이전트 지침)$/ }).last();
  const visible = async (locator: ReturnType<Page['locator']>): Promise<boolean> => locator.isVisible().catch(() => false);
  if (await visible(instructions)) {
    await expandAgentPanelIfNeeded(page);
    return;
  }
  if (!(await visible(agent))) {
    // Older Flow builds expose only the already-active Agent composer.
    if (await visible(page.locator('button').filter({ hasText: /tune/ }).first())) {
      await expandAgentPanelIfNeeded(page);
      return;
    }
    throw new FlowError('UI_NOT_FOUND', 'Flow Agent composer is not available. Nothing was sent.');
  }
  const pressed = (await agent.getAttribute('aria-pressed')) ?? (await agent.getAttribute('aria-checked'));
  if (pressed === 'true') {
    await expandAgentPanelIfNeeded(page);
    return;
  }
  const standardSettings = page.getByRole('button', { name: 'Settings trigger', exact: true }).last();
  if (pressed !== 'false' && !(await visible(standardSettings))) {
    throw new FlowError('UI_NOT_FOUND', 'Cannot determine Flow Agent mode safely. Nothing was sent.');
  }
  await agent.click({ timeout: 10_000 });
  // React/Angular needs a render turn after toggling; do not mistake the old
  // standard-mode arrow_forward for a ready Agent composer.
  for (let i = 0; i < 40; i++) {
    if (await visible(instructions) || (await agent.getAttribute('aria-pressed').catch(() => null)) === 'true') {
      await expandAgentPanelIfNeeded(page);
      return;
    }
    await page.waitForTimeout(250);
  }
  throw new FlowError('UI_NOT_FOUND', 'Flow did not enter Agent mode. Nothing was sent.');
}

/** Composer settings has an accessible name even when its icon has no text. */
export async function openAgentSettings(page: Page): Promise<void> {
  const icon = page.locator('button').filter({ hasText: /tune/ }).first();
  if (await icon.isVisible().catch(() => false)) {
    await icon.click({ timeout: 10_000 });
    return;
  }
  const named = page.getByRole('button', { name: /^(Agent settings|Settings|에이전트 설정|설정)$/ }).last();
  if (!(await named.isVisible().catch(() => false))) {
    throw new FlowError('UI_NOT_FOUND', 'Agent settings button is not visible in Flow. Nothing was sent.');
  }
  await named.click({ timeout: 10_000 });
}
