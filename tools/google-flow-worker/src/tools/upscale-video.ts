import path from 'node:path';
import { z } from 'zod';
import type { AppContext } from '../context.js';
import { FlowError } from '../lib/errors.js';
import { ensureOnProject } from '../flow/project.js';
import { mediaTiles, mediaTileCentre, tryDownload, tileUrl, type MediaTile } from '../flow/media.js';
import { sleep, isVisible, pressEscape } from '../flow/ui.js';
import { defineTool } from '../server/tool.js';

export const upscaleVideo = defineTool({
  name: 'flow_upscale_video',
  title: 'Upscale video to 1080p in Google Flow',
  description:
    'Trigger Google Flow\'s cloud upscale to 1080p resolution for a generated video and download the resulting 1080p MP4 file.',
  input: {
    project_url: z.url().describe('Google Flow project URL'),
    output_dir: z.string().describe('Local output directory for downloaded 1080p file'),
    media_id: z.string().optional().describe('Media ID or title prefix of the video to upscale'),
    auto_confirm: z.boolean().default(true),
  },
  output: {
    status: z.enum(['completed', 'failed']),
    files: z.array(z.string()).describe('Paths to downloaded 1080p video files'),
    media_ids: z.array(z.string()),
  },
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true },
  async run(ctx: AppContext, args) {
    const page = await ctx.session.ensureConnected();
    await ensureOnProject(page, args.project_url, ctx.log);
    await sleep(2000);

    const baselineTiles = (await mediaTiles(page)).filter((t) => t.kind === 'video');
    const baselineUrls = new Set(baselineTiles.map((t) => t.url));

    let tile: MediaTile | undefined;
    if (args.media_id) {
      tile = baselineTiles.find(
        (t) => (t.uuid && t.uuid.includes(args.media_id!)) || t.url.includes(args.media_id!) || t.title.includes(args.media_id!),
      );
    }
    if (!tile && baselineTiles.length > 0) {
      tile = baselineTiles[0];
    }

    let triggered = false;

    // 1. Check for Upscale button in the Agent composer chat
    const chatUpscale = page.locator('button').filter({ hasText: /Upscale|1080p|업스케일|Nâng cấp/i }).last();
    if (await isVisible(chatUpscale)) {
      ctx.log.info('upscale: clicking upscale button in composer chat');
      await chatUpscale.click().catch(() => undefined);
      triggered = true;
      await sleep(1500);
    }

    // 2. If not triggered, inspect the target tile's hover context menu
    if (!triggered && tile) {
      const centre = await mediaTileCentre(page, tile.url);
      if (centre) {
        await page.mouse.move(centre.x, centre.y);
        await sleep(800);
        const menuBtn = page.locator('button').filter({ hasText: /more_vert/ }).first();
        if (await isVisible(menuBtn)) {
          await menuBtn.click().catch(() => undefined);
          await sleep(800);
          const upscaleItem = page.locator('[role="menuitem"]').filter({ hasText: /Upscale|1080p|업스케일|Nâng cấp/i }).first();
          if (await isVisible(upscaleItem)) {
            ctx.log.info('upscale: clicking menu item Upscale');
            await upscaleItem.click().catch(() => undefined);
            triggered = true;
            await sleep(1500);
          } else {
            await pressEscape(page);
          }
        }
      }
    }

    // 3. If not triggered, try opening preview modal by clicking tile
    if (!triggered && tile) {
      const centre = await mediaTileCentre(page, tile.url);
      if (centre) {
        await page.mouse.click(centre.x, centre.y);
        await sleep(1500);
        const modalUpscale = page.locator('button').filter({ hasText: /Upscale|1080p|업스케일|Nâng cấp/i }).first();
        if (await isVisible(modalUpscale)) {
          ctx.log.info('upscale: clicking upscale button in video preview modal');
          await modalUpscale.click().catch(() => undefined);
          triggered = true;
          await sleep(1500);
        }
        await pressEscape(page);
        await sleep(600);
      }
    }

    // 4. If still not triggered, send Agent instruction command in chat composer
    if (!triggered) {
      ctx.log.info('upscale: sending upscale instruction to agent chat');
      const composer = page.locator('textarea, [contenteditable="true"]').last();
      if (await isVisible(composer)) {
        await composer.fill('Upscale the last generated video to 1080p resolution.');
        await sleep(600);
        const sendBtn = page.locator('button').filter({ hasText: /arrow_forward/ }).last();
        if (await isVisible(sendBtn)) {
          await sendBtn.click().catch(() => undefined);
          triggered = true;
          await sleep(2000);
        }
      }
    }

    // Poll for the completed 1080p video (up to 180s)
    const t0 = Date.now();
    const downloaded: { path: string; media_id: string }[] = [];

    while (Date.now() - t0 < 180_000) {
      await sleep(4000);

      const currentTiles = (await mediaTiles(page)).filter((t) => t.kind === 'video');
      const newTiles = currentTiles.filter((t) => !baselineUrls.has(t.url));

      if (newTiles.length > 0 && newTiles[0]) {
        const newTile = newTiles[0];
        let attempt = await tryDownload(page.context(), `${newTile.url}=mm,37,15`, 'video', args.output_dir, '1080p_flow');
        if (attempt.outcome !== 'ok') {
          attempt = await tryDownload(page.context(), tileUrl(newTile), 'video', args.output_dir, '1080p_flow');
        }
        if (attempt.outcome === 'ok') {
          downloaded.push({ path: attempt.file.path, media_id: attempt.file.media_id });
          break;
        }
      } else if (tile) {
        const direct1080 = await tryDownload(page.context(), `${tile.url}=mm,37,15`, 'video', args.output_dir, '1080p_flow');
        if (direct1080.outcome === 'ok') {
          downloaded.push({ path: direct1080.file.path, media_id: direct1080.file.media_id });
          break;
        }
      }
    }

    if (downloaded.length === 0) {
      throw new FlowError('GENERATION_TIMEOUT', 'Google Flow cloud upscale timed out or video was not ready.');
    }

    return {
      status: 'completed' as const,
      files: downloaded.map((d) => d.path),
      media_ids: downloaded.map((d) => d.media_id),
    };
  },
});
