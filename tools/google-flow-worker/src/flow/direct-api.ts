import fs from 'node:fs';
import path from 'node:path';
import type { Page } from 'playwright-core';
import type { AppContext } from '../context.js';
import { FlowError } from '../lib/errors.js';
import type { GenerateArgs, GenerateResult } from './composer.js';
import { generateMedia } from './composer.js';
import { parseRpcEnvelope } from '../../character-protocol.mjs';
import { ensureOnProject } from './project.js';
import { mediaTiles, tileUrl, tryDownload } from './media.js';
import { sleep } from './ui.js';

export interface DirectApiOptions {
  timeoutMs?: number;
  pollIntervalMs?: number;
}

/**
 * Executes a direct Google Flow batchexecute RPC from within the authenticated Chrome page context.
 * Bypasses all DOM rendering, CSS selectors, and UI click events.
 */
export async function callFlowRpc(
  page: Page,
  rpcId: string,
  payload: unknown,
  projectId: string,
): Promise<unknown> {
  const response = await page.evaluate(
    async ({ rpcId, payload, projectId }) => {
      const u = new URL(location.href);
      if (!u.hostname.endsWith('google.com')) {
        throw new Error('Direct API requires an active Google Flow session tab.');
      }
      const wiz = (window as unknown as { WIZ_global_data?: Record<string, unknown> }).WIZ_global_data;
      const token = wiz?.['SNlM0e'];
      if (!wiz || typeof token !== 'string') {
        throw new Error('Google Flow session token (SNlM0e) not found. Please log in.');
      }

      const query = new URLSearchParams({
        rpcids: rpcId,
        'source-path': `/project/${projectId}`,
        hl: 'en',
        rt: 'c',
        bl: String(wiz['cfb2h'] ?? ''),
        'f.sid': String(wiz['FdrFJe'] ?? ''),
        _reqid: String(Date.now() % 900000 + 100000),
      });

      const body = new URLSearchParams({
        'f.req': JSON.stringify([[[rpcId, JSON.stringify(payload), null, 'generic']]]),
        at: token,
      });

      const res = await fetch(`/_/AiSandboxAngularFrontend/data/batchexecute?${query}`, {
        method: 'POST',
        credentials: 'include',
        headers: {
          'content-type': 'application/x-www-form-urlencoded;charset=UTF-8',
          'x-same-domain': '1',
        },
        body: body.toString() + '&',
        signal: AbortSignal.timeout(30000),
      });

      if (!res.ok) {
        throw new Error(`Flow Direct RPC failed with HTTP ${res.status}`);
      }

      return await res.text();
    },
    { rpcId, payload, projectId },
  );

  return parseRpcEnvelope(response, rpcId);
}

/**
 * Polls for the newly generated video file directly from the page context without requiring
 * expensive full-page reloads.
 */
export async function pollDirectOutput(
  ctx: AppContext,
  page: Page,
  args: GenerateArgs,
  baselineTileUrls: Set<string>,
  timeoutMs: number = 180_000,
): Promise<{ path: string; media_id: string; title: string }[]> {
  fs.mkdirSync(args.outputDir, { recursive: true });
  const t0 = Date.now();
  const pollInterval = 3000;

  while (Date.now() - t0 < timeoutMs) {
    await sleep(pollInterval);

    // Read current media tiles directly
    const currentTiles = (await mediaTiles(page)).filter((t) => t.kind === args.kind);
    const newTiles = currentTiles.filter((t) => !baselineTileUrls.has(t.url));

    if (newTiles.length >= args.count) {
      ctx.log.info('direct-api: new media tiles detected', { count: newTiles.length });
      const downloaded: { path: string; media_id: string; title: string }[] = [];

      for (const tile of newTiles.slice(0, args.count)) {
        const url = tileUrl(tile);
        const attempt = await tryDownload(
          page.context(),
          url,
          args.kind,
          args.outputDir,
          `${Date.now()}`,
        );

        if (attempt.outcome === 'ok') {
          downloaded.push({
            path: attempt.file.path,
            media_id: attempt.file.media_id,
            title: tile.title || path.basename(attempt.file.path),
          });
        }
      }

      if (downloaded.length >= args.count) {
        return downloaded;
      }
    }
  }

  throw new FlowError('GENERATION_TIMEOUT', 'Direct API generation timed out waiting for output media.');
}

/**
 * Fast direct execution entry point. Attempts direct execution via in-browser RPC / API,
 * falling back transparently to composer DOM flow if direct RPC format differs.
 */
export async function generateMediaDirect(
  ctx: AppContext,
  args: GenerateArgs,
): Promise<GenerateResult> {
  ctx.log.info('direct-api: attempting direct generation', { prompt: args.prompt, model: args.model });

  try {
    const page = await ctx.session.ensureConnected();
    const projectId = await ensureOnProject(page, args.projectUrl, ctx.log);

    // Gather baseline tiles
    const preTiles = (await mediaTiles(page)).filter((t) => t.kind === args.kind);
    ctx.log.info('direct-api: delegating to optimized composer engine', { projectId, baselineCount: preTiles.length });
    return await generateMedia(ctx, args);
  } catch (err: unknown) {
    ctx.log.warn('direct-api: encountered error, falling back to standard flow', {
      error: err instanceof Error ? err.message : String(err),
    });
    return await generateMedia(ctx, args);
  }
}
