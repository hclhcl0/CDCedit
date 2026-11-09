// UI ingredient selection adapted from RohaanA/google-flow-mcp, MIT (LICENSE.characters).
import type { Locator, Page } from 'playwright-core';
import type { FlowCharacter } from '../../character-protocol.mjs';
import { FlowError } from '../lib/errors.js';
import { labelButton, namedButton, sleep } from './ui.js';

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Scope verification to the composer that owns our input, never the project gallery. */
async function composer(page: Page, input: Locator): Promise<Locator> {
  const found = await input.evaluate((editor) => {
    document.querySelectorAll('[data-concat-character-composer]').forEach((el) => el.removeAttribute('data-concat-character-composer'));
    let el: Element | null = editor;
    while (el && !el.querySelector('button[aria-label*="prompt box"], button[aria-label*="프롬프트 상자"]')) el = el.parentElement;
    if (!el || el === document.body || el === document.documentElement) return false;
    el.setAttribute('data-concat-character-composer', '1');
    return true;
  });
  if (!found) throw new FlowError('REFERENCE_NOT_ATTACHED', 'Cannot identify the Flow character composer. Nothing was sent.');
  return page.locator('[data-concat-character-composer]');
}

export async function verifyCharacterIngredients(page: Page, input: Locator, characters: FlowCharacter[]): Promise<void> {
  const root = await composer(page, input);
  const chips = root.locator('flow-character-ingredient-chip');
  const count = await chips.count();
  if (count !== characters.length) {
    throw new FlowError(
      'REFERENCE_NOT_ATTACHED',
      `Could not verify every saved character ingredient (found ${count}/${characters.length}). Nothing was sent.`
    );
  }
}

export async function attachCharacterIngredients(page: Page, input: Locator, characters: FlowCharacter[]): Promise<void> {
  const root = await composer(page, input);

  // If stale character ingredients remain in the composer, clear prompt to reset cleanly
  const staleCount = await root.locator('flow-character-ingredient-chip').count();
  if (staleCount > 0 && staleCount !== characters.length) {
    const clearBtn = page.locator('button[aria-label*="Clear prompt"], button[aria-label*="프롬프트 지우기"]').first();
    if (await clearBtn.isVisible().catch(() => false)) {
      await clearBtn.click().catch(() => undefined);
      await sleep(500);
    }
  }

  for (const item of characters) {
    const before = await root.locator('flow-character-ingredient-chip').count();

    // Check if asset menu is already open
    const isMenuOpen = (await page.locator('.asset-item-container:visible').count()) > 0;
    if (!isMenuOpen) {
      const attachBtn = page.locator('button[aria-label*="prompt box"], button[aria-label*="프롬프트 상자"]').first();
      await attachBtn.click();
      await sleep(400);
    }

    const options = page.locator('.asset-item-container button[role="option"]:visible');
    await options.first().waitFor({ state: 'visible', timeout: 10000 });

    const search = page.getByPlaceholder('Search assets', { exact: true });
    if (await search.isVisible()) {
      await search.fill(item.name);
      await sleep(600);
    }

    // Both exact name and Character type are required; a portrait Image is insufficient.
    const candidate = options.filter({
      has: page.locator('.asset-title', { hasText: new RegExp(`^\\s*${escape(item.name)}\\s*$`, 'i') }),
    }).filter({
      has: page.locator('.type-subtitle', { hasText: /^(?:Character|캐릭터)$/i }),
    });

    if ((await candidate.count()) !== 1) {
      throw new FlowError('REFERENCE_NOT_ATTACHED', `Cannot uniquely select saved character ${item.name}. Nothing was sent.`);
    }

    await candidate.click();
    await sleep(500);

    // If detail pane with "Add to prompt" appears, click it
    const addBtn = page.locator('button.detail-add-to-prompt-btn, button:has-text("Add to prompt"), button:has-text("프롬프트에 추가")').first();
    if (await addBtn.isVisible().catch(() => false)) {
      await addBtn.click();
      await sleep(600);
    }

    // Wait up to 5s for the character ingredient chip count to increase
    let attachedOk = false;
    for (let t = 0; t < 10; t++) {
      if ((await root.locator('flow-character-ingredient-chip').count()) >= before + 1) {
        attachedOk = true;
        break;
      }
      await sleep(300);
    }

    if (!attachedOk) {
      throw new FlowError('REFERENCE_NOT_ATTACHED', `Flow did not attach the saved character ingredient ${item.name}. Nothing was sent.`);
    }
  }

  await verifyCharacterIngredients(page, input, characters);
}
