import type { Page } from 'playwright-core';
import type { AppContext } from '../context.js';
import { FlowError } from '../lib/errors.js';
import { ensureOnProject } from './project.js';
import { sleep } from './ui.js';

export interface AgentInstructionItem {
  title: string;
  text: string;
  imageName?: string | undefined;
}

export interface SyncAgentInstructionsArgs {
  projectUrl?: string | undefined;
  instructions: AgentInstructionItem[];
}

export async function syncProjectAgentInstructions(
  ctx: AppContext,
  page: Page,
  args: SyncAgentInstructionsArgs,
): Promise<{ success: boolean; syncedCount: number; message: string }> {
  if (args.projectUrl && args.projectUrl.trim()) {
    await ensureOnProject(page, args.projectUrl.trim(), ctx.log);
  } else if (!page.url().includes('/project/')) {
    throw new FlowError('PROJECT_NOT_FOUND', 'Hãy mở hoặc nhập URL dự án Google Flow trước khi đồng bộ Agent instructions.');
  }

  // 1. Open Agent instructions drawer
  const sparkBtn = page.locator('button[aria-label*="Agent instructions"], button:has-text("article_spark")').first();
  if ((await sparkBtn.count()) === 0) {
    throw new FlowError('UI_NOT_FOUND', 'Nút Agent instructions không tìm thấy trong giao diện Flow.');
  }

  // Check if panel is already open
  let addBtn = page.locator('.add-instruction-button, button:has-text("Add instruction")').first();
  if (!(await addBtn.isVisible().catch(() => false))) {
    await sparkBtn.click();
    await sleep(1000);
  }

  addBtn = page.locator('.add-instruction-button, button:has-text("Add instruction")').first();
  await addBtn.waitFor({ timeout: 10_000 });

  let synced = 0;

  for (const item of args.instructions) {
    ctx.log.info('syncing agent instruction', { title: item.title });

    // Check if an existing card matches this title
    const existingCards = page.locator('.instruction-card-container');
    const count = await existingCards.count();
    let card = null;

    for (let i = 0; i < count; i++) {
      const c = existingCards.nth(i);
      const val = await c.locator('input').inputValue().catch(() => '');
      if (val.trim().toLowerCase() === item.title.trim().toLowerCase()) {
        card = c;
        break;
      }
    }

    if (!card) {
      // Click Add instruction
      await addBtn.click();
      await sleep(800);
      card = page.locator('.instruction-card-container').last();
    }

    // Fill Title
    const titleInput = card.locator('input[placeholder="Instruction title"], input.editable-text-input').first();
    if (await titleInput.isVisible().catch(() => false)) {
      await titleInput.click();
      await page.keyboard.press('Control+A');
      await page.keyboard.press('Backspace');
      await titleInput.fill(item.title);
      await sleep(300);
    }

    // Fill Guideline Text
    const descInput = card.locator('textarea[placeholder*="guideline"], textarea.description-input').first();
    if (await descInput.isVisible().catch(() => false)) {
      await descInput.click();
      await page.keyboard.press('Control+A');
      await page.keyboard.press('Backspace');
      await descInput.fill(item.text);
      await sleep(300);
    }

    // Handle Reference Image if specified
    if (item.imageName) {
      const refBtn = card.locator('button').filter({ hasText: /Reference/i }).first();
      if (await refBtn.isVisible().catch(() => false)) {
        await refBtn.click();
        await sleep(1000);

        const dialog = page.locator('mat-dialog-container, [role="dialog"]').first();
        if (await dialog.isVisible().catch(() => false)) {
          const cleanName = item.imageName.replace(/\.[^/.]+$/, '').trim();
          const option = dialog.locator('button').filter({
            hasText: new RegExp(cleanName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'i'),
          }).first();

          if ((await option.count().catch(() => 0)) > 0) {
            await option.click();
            await sleep(500);

            const confirmBtn = dialog.locator('button:has-text("Add to instruction")').first();
            if (await confirmBtn.isVisible().catch(() => false)) {
              await confirmBtn.click();
              await sleep(600);
            }
            ctx.log.info('attached reference image to agent instruction', { imageName: item.imageName });
          } else {
            // Press escape to close image picker if image not found
            await page.keyboard.press('Escape');
            await sleep(400);
          }
        }
      }
    }

    // Ensure toggle switch is active (ON)
    const toggle = card.locator('[role="switch"], mat-slide-toggle button').first();
    if (await toggle.isVisible().catch(() => false)) {
      const checked = (await toggle.getAttribute('aria-checked')) === 'true';
      if (!checked) {
        await toggle.click();
        await sleep(300);
      }
    }

    synced++;
  }

  // Click Done to save
  const doneBtn = page.locator('button:has-text("Done")').first();
  if (await doneBtn.isVisible().catch(() => false)) {
    await doneBtn.click();
    await sleep(800);
  }

  return {
    success: true,
    syncedCount: synced,
    message: `Đã đồng bộ thành công ${synced} chỉ dẫn vào Agent instructions của Flow.`,
  };
}
