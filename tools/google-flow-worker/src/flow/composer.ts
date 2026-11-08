import fs from 'node:fs';
import path from 'node:path';
import type { Locator, Page } from 'playwright-core';
import { takeScreenshot } from '../browser/screenshot.js';
import type { AppContext } from '../context.js';
import { FlowError } from '../lib/errors.js';
import type { Job } from './jobs.js';
import { ensureAgentMode, openAgentSettings } from './agent-mode.js';
import { IMAGE_MODELS, type ModelId, label, modelLabel, matchesModel } from './labels.js';
import { type MediaKind, downloadedPrefixes, mediaTiles, tileUrl, tryDownload } from './media.js';
import { ensureOnProject } from './project.js';
import { listCharacters } from './characters.js';
import { attachCharacterIngredients, verifyCharacterIngredients } from './character-ingredients.js';
import { resolveCharacterReferences, type CharacterReference } from '../../character-protocol.mjs';
import {
  bodyText,
  firstInRegion,
  iconButton,
  isVisible,
  labelButton,
  namedButton,
  pressEscape,
  sleep,
  waitStable,
} from './ui.js';

// Flow's agent chat composer (right-hand panel): settings popover (tune), attach menu (add_2), send (arrow_forward),
// an optional approval card, and the media grid where outputs appear. This module owns that whole flow.

export interface GenerateArgs {
  kind: MediaKind;
  prompt: string;
  model: ModelId;
  ratio: string;
  duration: number | undefined;
  count: number;
  referenceImages: string[];
  characterReferences?: CharacterReference[];
  projectUrl: string;
  outputDir: string;
  autoConfirm: boolean;
  resume: boolean;
  jobId: string | undefined;
  editMode: boolean;
}

// Type alias so the result can be stored as a job record (Record<string, unknown>)
export type GenerateResult = {
  status: 'ready_for_confirmation' | 'completed';
  job_id: string;
  files: string[];
  media_ids: string[];
  references_attached: number;
  characters_attached?: number;
  approval_text: string;
  model: string;
  ratio: string;
  duration?: number;
  elapsed_ms: number;
  prompt: string;
  screenshot?: string;
};

const APPROVAL_TIMEOUT_MS = 120_000;
const RESUME_APPROVAL_TIMEOUT_MS = 30_000;
const SEND_ATTEMPTS = 30;
let lastGenerationFinishedAt = 0;

async function humanTypePrompt(page: Page, input: Locator, text: string): Promise<void> {
  await input.click({ force: true }).catch(() => undefined);
  await sleep(200);

  // Reliably set text into the rich-text / contenteditable composer with proper native events
  await page.evaluate((val) => {
    const el = document.querySelector('[contenteditable="true"]:not([aria-hidden="true"]), textarea') as HTMLElement;
    if (!el) return;
    el.focus();
    if (el.isContentEditable) {
      const range = document.createRange();
      range.selectNodeContents(el);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      document.execCommand('delete', false);
      document.execCommand('insertText', false, val);
      el.dispatchEvent(new Event('input', { bubbles: true, cancelable: true }));
      el.dispatchEvent(new Event('change', { bubbles: true, cancelable: true }));
    } else if (el instanceof HTMLTextAreaElement) {
      el.value = val;
      el.dispatchEvent(new Event('input', { bubbles: true, cancelable: true }));
      el.dispatchEvent(new Event('change', { bubbles: true, cancelable: true }));
    }
  }, text);

  await sleep(250);

  // Fallback if evaluate didn't populate text
  const current = (await input.innerText().catch(() => '')).trim();
  if (!current && text.trim()) {
    await input.click().catch(() => undefined);
    await page.keyboard.insertText(text);
    await sleep(200);
  }
}

export async function generateMedia(ctx: AppContext, args: GenerateArgs): Promise<GenerateResult> {
  const sinceLast = Date.now() - lastGenerationFinishedAt;
  if (lastGenerationFinishedAt > 0 && sinceLast < 3500) {
    await sleep(3500 - sinceLast);
  }
  const started = Date.now();
  const page = await ctx.session.ensureConnected();
  const shots = path.join(ctx.config.stateDir, 'screenshots');
  const modelName = modelLabel(args.model);
  const projectId = await ensureOnProject(page, args.projectUrl, ctx.log);

  if (args.resume) return resumeGeneration(ctx, page, args, started);

  const requestedCharacters=args.characterReferences??[];
  if (requestedCharacters.length && (args.kind!=='video' || args.model!=='omni-flash-1.2' || args.editMode)) {
    throw new FlowError('MODEL_NOT_AVAILABLE','Saved character + voice reuse requires a new Omni Flash 1.2 video. Nothing was sent.');
  }
  const characters=requestedCharacters.length
    ? resolveCharacterReferences(await listCharacters(page,projectId),requestedCharacters,projectId) : [];

  await ensureComposer(ctx, page, true);
  await applyDefaults(ctx, page, args.projectUrl, args.kind, args.ratio, modelName, args.count);
  await ensureComposer(ctx, page, args.editMode);

  // Attachments are counted, not diffed: each one has to raise the composer's thumbnail count past what the
  // previous references already put there. Anything already in the composer is the origin, so a stale thumbnail
  // cannot make a reference look attached.
  const origin = (await composerMedia(page)).length;
  let attached = 0;
  for (const ref of args.referenceImages) {
    const want = origin + attached + 1;
    if ((await attachReference(ctx, page, ref, want)) || (await attachReference(ctx, page, ref, want))) attached++;
  }
  // Sending without the reference makes the agent pick an arbitrary project image as the first frame: stop before credits
  if (args.referenceImages.length > 0 && attached < args.referenceImages.length) {
    await clearComposer(ctx, page);
    throw new FlowError(
      'REFERENCE_NOT_ATTACHED',
      `${attached}/${args.referenceImages.length} reference images attached; nothing was sent`,
      { attached, requested: args.referenceImages.length },
      await takeScreenshot(page, shots, 'reference-not-attached'),
    );
  }

  const bindings=characters.map((item,index)=>`Scene character ${JSON.stringify(requestedCharacters[index]?.local_name || item.name)} uses the attached saved Flow character ${JSON.stringify(item.name)} and its existing voice.`).join('\n');
  const instruction = buildInstruction(args, modelName, attached)+(bindings ? `\nSaved character bindings (preserve these identities):\n${bindings}` : '');
  const bodyBefore = await bodyText(page);
  const input = composerInput(page);
  await humanTypePrompt(page, input, instruction);
  await sleep(400);
  if (characters.length) {
    // Attach after typing: fill() can remove native character chips from the editor.
    // No subsequent fill/retry may silently replace a character with its portrait.
    await attachCharacterIngredients(page,input,characters);
  }

  const base = {
    references_attached: attached,
    characters_attached: characters.length,
    model: modelName,
    ratio: args.ratio,
    prompt: instruction,
    ...(args.duration !== undefined ? { duration: args.duration } : {}),
  };

  if (!args.autoConfirm) {
    const screenshot = await takeScreenshot(page, shots, `${args.kind}-ready`);
    return {
      status: 'ready_for_confirmation',
      job_id: '',
      files: [],
      media_ids: [],
      approval_text: '',
      elapsed_ms: Date.now() - started,
      ...base,
      ...(screenshot ? { screenshot } : {}),
    };
  }

  // The baseline has to describe the grid as it was before sending, and the grid is still rendering right
  // after navigation — so wait for a stable count, then take it once. Re-reading it later would swallow a
  // fast generation into the baseline and leave the job waiting for an output it already had.
  await waitStable(async () => (await mediaTiles(page)).length);
  if (characters.length) await verifyCharacterIngredients(page,input,characters);
  const preTiles = (await mediaTiles(page)).filter((t) => t.kind === args.kind);
  const baselineUrls = new Set(preTiles.map((t) => t.url));
  const job = ctx.jobs.create({
    kind: args.kind,
    phase: 'sending',
    project_url: args.projectUrl,
    output_dir: args.outputDir,
    prompt: instruction,
    model: modelName,
    baseline_tiles: preTiles.length,
  });
  await send(ctx, page, input, shots, characters.length ? () => verifyCharacterIngredients(page,input,characters) : undefined);
  ctx.jobs.update(job.job_id, { phase: 'sent' });

  const approvalText = await awaitApproval(
    ctx,
    page,
    bodyBefore,
    args.kind,
    job.baseline_tiles,
    APPROVAL_TIMEOUT_MS,
    characters.length === 0,
    baselineUrls,
  );
  const files = await awaitOutputs(ctx, page, args, job.job_id, job.baseline_tiles, bodyBefore, shots, baselineUrls);
  ctx.jobs.update(job.job_id, { outputs: files.map((f) => ({ media_id: f.media_id, title: f.title })) });
  const result: GenerateResult = {
    status: 'completed',
    job_id: job.job_id,
    files: files.map((f) => f.path),
    media_ids: files.map((f) => f.media_id),
    approval_text: approvalText,
    elapsed_ms: Date.now() - started,
    ...base,
  };
  ctx.jobs.update(job.job_id, { phase: 'done', result });
  lastGenerationFinishedAt = Date.now();
  return result;
}

async function resumeGeneration(
  ctx: AppContext,
  page: Page,
  args: GenerateArgs,
  started: number,
): Promise<GenerateResult> {
  const shots = path.join(ctx.config.stateDir, 'screenshots');
  const job = args.jobId
    ? ctx.jobs.get(args.jobId)
    : ctx.jobs.findLatest(
        (j) =>
          j.kind === args.kind &&
          j.project_url === args.projectUrl &&
          j.output_dir === args.outputDir &&
          j.phase !== 'failed',
      );
  if (!job) {
    throw new FlowError('GENERATION_TIMEOUT', 'no in-flight generation to resume for this project/output_dir', {
      reason: 'no_job',
      project_url: args.projectUrl,
      output_dir: args.outputDir,
    });
  }
  if (job.phase === 'done' && job.result) {
    ctx.log.info('resume: job already completed', { job_id: job.job_id });
    return { ...(job.result as unknown as GenerateResult), elapsed_ms: Date.now() - started };
  }
  await pressEscape(page);
  const bodyBefore = await bodyText(page);
  const approvalText = await awaitApproval(
    ctx,
    page,
    bodyBefore,
    args.kind,
    job.baseline_tiles,
    RESUME_APPROVAL_TIMEOUT_MS,
    false,
  );
  const files = await awaitOutputs(ctx, page, args, job.job_id, job.baseline_tiles, bodyBefore, shots);
  ctx.jobs.update(job.job_id, { outputs: files.map((f) => ({ media_id: f.media_id, title: f.title })) });
  const result: GenerateResult = {
    status: 'completed',
    job_id: job.job_id,
    files: files.map((f) => f.path),
    media_ids: files.map((f) => f.media_id),
    references_attached: 0,
    approval_text: approvalText,
    model: job.model,
    ratio: args.ratio,
    ...(args.duration !== undefined ? { duration: args.duration } : {}),
    elapsed_ms: Date.now() - started,
    prompt: job.prompt,
  };
  ctx.jobs.update(job.job_id, { phase: 'done', result });
  lastGenerationFinishedAt = Date.now();
  return result;
}

function composerInput(page: Page): Locator {
  return page.locator('[contenteditable="true"]:visible, textarea:visible').first();
}

function buildInstruction(args: GenerateArgs, modelName: string, attached: number): string {
  const prompt = args.prompt.trim();
  if (args.editMode) {
    return `Edit the last generated video (keep the same ${args.duration ?? 8}-second length and ${args.ratio} ratio): ${prompt}`;
  }
  return prompt;
}

async function closeSidePanels(page: Page): Promise<void> {
  // The settings popover closes via Save; the X next to it would close the whole chat panel
  if (await isVisible(page.getByText(label('agentSettings')).first())) {
    await labelButton(page, 'save', 'exact')
      .first()
      .click()
      .catch(() => undefined);
    await sleep(800);
  }
  await pressEscape(page);
}

// Thumbnails attached to the composer sit right above the composer input box; grid tiles and dialogs do not
async function composerMedia(page: Page): Promise<string[]> {
  return page
    .evaluate(() => {
      const inputs = Array.from(document.querySelectorAll<HTMLElement>('[contenteditable="true"], textarea')).filter(
        (el) => {
          const r = el.getBoundingClientRect();
          return r.width > 40 && r.height > 10;
        },
      );
      const ir = inputs[0]?.getBoundingClientRect();
      return Array.from(document.querySelectorAll('img'))
        .filter((i) => {
          if (i.closest('flow-image-tile, flow-video-tile, flow-grid-tile-container, [role="dialog"]')) {
            return false;
          }
          const r = i.getBoundingClientRect();
          const src = i.currentSrc || i.src;
          if (
            r.width < 20 ||
            r.width > 160 ||
            r.height < 20 ||
            r.height > 160 ||
            /zero_states|\/website\/flow\/|\.svg(\?|$)|gstatic\.com\/gb\/|lh3\.googleusercontent\.com\/a|lh3\.google\.com\/u\//.test(
              src,
            )
          ) {
            return false;
          }
          if (ir) {
            return (
              r.top >= ir.top - 180 &&
              r.bottom <= ir.bottom + 80 &&
              r.left >= ir.left - 80 &&
              r.right <= ir.right + 160
            );
          }
          return r.y > window.innerHeight * 0.6;
        })
        .map((i) => i.currentSrc || i.src);
    })
    .catch(() => []);
}

async function clearComposer(ctx: AppContext, page: Page): Promise<void> {
  const clear = labelButton(page, 'clearPrompt').first();
  if (await isVisible(clear)) {
    await clear.click().catch(() => undefined);
    await sleep(500);
  }
  for (let i = 0; i < 6 && (await composerMedia(page)).length > 0; i++) {
    const cancel = page
      .locator('button')
      .filter({ hasText: /^\s*(?:cancel|close)\s*$/ })
      .last();
    if (!(await isVisible(cancel))) break;
    await cancel.click().catch(() => undefined);
    await sleep(400);
  }
  if ((await composerMedia(page)).length > 0) ctx.log.warn('composer still has attachments after clear');
  if (await page.locator('flow-character-ingredient-chip:visible').count()) {
    throw new FlowError('REFERENCE_NOT_ATTACHED','Could not clear previous saved character ingredients. Nothing was sent.');
  }
}

// A fresh chat session per request stops the agent from reusing the previous request's attachment as first frame
async function newSession(ctx: AppContext, page: Page): Promise<void> {
  const b = page
    .locator('button')
    .filter({ hasText: /edit_square|새로운 세션|New session/ })
    .first();
  if (await isVisible(b)) {
    await b.click().catch(() => undefined);
    await sleep(1500);
    ctx.log.info('started a new agent session');
  }
}

async function ensureComposer(ctx: AppContext, page: Page, keepSession: boolean): Promise<void> {
  await ensureAgentMode(page);
  await closeSidePanels(page);
  if (!(await isVisible(iconButton(page, 'send').first()))) {
    ctx.log.info('composer not visible, reloading project page');
    await page.reload({ waitUntil: 'domcontentloaded' });
    await sleep(5000);
    await ensureAgentMode(page);
    await closeSidePanels(page);
    if (!(await isVisible(iconButton(page, 'send').first()))) {
      throw new FlowError(
        'UI_NOT_FOUND',
        'agent composer (arrow_forward button) not found on the project page',
        {},
        await takeScreenshot(page, path.join(ctx.config.stateDir, 'screenshots'), 'no-composer'),
      );
    }
  }
  await clearComposer(ctx, page);
  if (!keepSession) await newSession(ctx, page);
}

// Agent settings popover: "confirm before generating" = Never, then the ratio / count / model for the media kind.
// The popover lists the image section first and the video section last, so same-label buttons are picked by position.
const appliedDefaults = new Map<string, string>();
async function applyDefaults(
  ctx: AppContext,
  page: Page,
  projectUrl: string,
  kind: MediaKind,
  ratio: string,
  modelName: string,
  count: number,
): Promise<void> {
  const key = `${projectUrl}|${kind}|${ratio}|${modelName}|${count}`;
  if (appliedDefaults.get(`${projectUrl}|${kind}`) === key) return;
  const pick = (loc: Locator): Locator => (kind === 'video' ? loc.last() : loc.first());
  await openAgentSettings(page);
  await page.getByText(label('agentSettings')).first().waitFor({ timeout: 10_000 });
  await sleep(500);
  const never = page.getByText(label('never', 'exact')).first();
  if (await isVisible(never)) {
    await never.click().catch(() => undefined);
    await sleep(300);
  }
  const ratioBtn = pick(page.locator('button').filter({ hasText: new RegExp(`${ratio.replace(':', '\\:')}\\s*$`) }));
  await ratioBtn.click().catch(() => ctx.log.warn('ratio button not found', { ratio }));
  const countBtn = pick(page.locator('button').filter({ hasText: new RegExp(`x${count}\\s*$`) }));
  await countBtn.click().catch(() => ctx.log.warn('count button not found', { count }));
  const modelRe =
    kind === 'video' ? /Omni|Veo/ : new RegExp(Object.values(IMAGE_MODELS).join('|').replace(/ /g, '\\s'));
  const modelBtn = pick(page.locator('button').filter({ hasText: modelRe }));
  const current = await modelBtn.innerText().catch(() => '');
  if (!matchesModel(current, modelName)) {
    await modelBtn.click();
    await sleep(700);
    const opt = modelName === 'Omni Flash 1.2'
      ? page.getByText(/Omni\s+(?:Flash\s+1\.2(?![\d.])|1\.2\s+Flash\b)/i).last()
      : page.getByText(modelName, { exact: false }).last();
    if (await isVisible(opt)) await opt.click();
    else {
      await pressEscape(page);
      throw new FlowError('MODEL_NOT_AVAILABLE', `Requested model ${modelName} is not visible in this Flow account. Nothing was sent.` + (modelName === 'Omni Flash' ? ' This is the legacy model. Select Omni Flash 1.2 in the project and add a NEW task; existing tasks keep the old model.' : ''), {model:modelName});
    }
    await sleep(400);
  }
  const selectedModel = await modelBtn.innerText().catch(() => '');
  if (!matchesModel(selectedModel, modelName)) {
    await pressEscape(page);
    throw new FlowError('MODEL_NOT_AVAILABLE', `Could not verify selected model ${modelName}; nothing was sent.`, {requested:modelName,selected:selectedModel});
  }
  await takeScreenshot(page, path.join(ctx.config.stateDir, 'screenshots'), `${kind}-defaults`);
  await labelButton(page, 'save', 'exact').first().click();
  await sleep(800);
  appliedDefaults.set(`${projectUrl}|${kind}`, key);
  ctx.log.info('agent defaults applied', { kind, ratio, model: modelName, count });
}

// attach button → Check existing library item → if not found, Upload media → confirm a new composer thumbnail
async function attachReference(ctx: AppContext, page: Page, filePath: string, expected: number): Promise<boolean> {
  const targetPath = filePath.replace(/^\\\\\?\\/, '');
  if (!fs.existsSync(targetPath)) {
    throw new FlowError('REFERENCE_NOT_ATTACHED', `reference image not found: ${targetPath}`, { path: targetPath });
  }
  const shots = path.join(ctx.config.stateDir, 'screenshots');
  // The retry after a false negative: the file did land, so uploading it again would only duplicate it
  if ((await composerMedia(page)).length >= expected) return true;
  // A previous failed attempt can leave the attach menu open; its overlay then swallows the click
  // and the button reports aria-expanded="true" forever (observed 2026-09-05).
  const attach = namedButton(page, 'attachToPrompt').first();
  if ((await attach.getAttribute('aria-expanded').catch(() => null)) === 'true') {
    await pressEscape(page, 500);
  }
  await attach.click();
  await sleep(600);

  const baseName = path.basename(targetPath);
  const nameNoExt = path.parse(targetPath).name;
  const escapedName = nameNoExt.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

  // Check if this image was already uploaded to Flow's project library/assets to prevent duplicate uploads
  let attachedFromLibrary = false;
  const existingItem = page.locator('button.asset-item').filter({
    hasText: new RegExp(escapedName, 'i')
  }).first();

  if ((await existingItem.count().catch(() => 0)) > 0) {
    ctx.log.info('reusing existing asset from Flow library', { baseName });
    await existingItem.click().catch(() => undefined);
    await sleep(500);
    const confirm = page.locator('button.detail-add-to-prompt-btn, button:has-text("Add to prompt"), button:has-text("프롬프트에 추가")').first();
    if (await confirm.isVisible().catch(() => false)) {
      await confirm.click().catch(() => undefined);
      attachedFromLibrary = true;
    }
  }

  if (!attachedFromLibrary) {
    const upload = labelButton(page, 'uploadMedia').last();
    await upload.waitFor({ timeout: 10_000 });
    const [chooser] = await Promise.all([page.waitForEvent('filechooser', { timeout: 15_000 }), upload.click()]);
    await chooser.setFiles(targetPath);
    ctx.log.info('reference file uploaded to Flow (new)', { filePath: targetPath });
    // Flow 2026-09-05: the upload opens the right-hand asset picker with the new file already selected,
    // so there is nothing to click in the list — only the '프롬프트에 추가' confirm attaches it to the composer.
    const confirm = labelButton(page, 'addToPrompt', 'exact').first();
    try {
      await confirm.waitFor({ timeout: 60_000 });
    } catch {
      await takeScreenshot(page, shots, 'ref-not-in-picker');
      await pressEscape(page);
      return false;
    }
    await confirm.click();
  }
  await sleep(800);
  const t0 = Date.now();
  while (Date.now() - t0 < 15_000) {
    await sleep(1000);
    // Landing is judged by the thumbnail count, not by a new URL appearing: re-uploading the same file makes
    // Flow hand back the URL it already used, so the set difference stayed empty and a reference that was
    // plainly attached (byblos-book 2026-09-05: the screenshot shows the thumbnail) was thrown away.
    if ((await composerMedia(page)).length >= expected) {
      if ((await page.locator('[role="dialog"]').count()) > 0) await pressEscape(page, 500);
      if ((await composerMedia(page)).length >= expected) return true;
    }
  }
  await takeScreenshot(page, shots, 'ref-not-attached');
  await pressEscape(page);
  return false;
}

// While an upload is still processing the composer silently ignores send; retry gently without rate-limiting
async function send(ctx: AppContext, page: Page, input: Locator, shots: string, verify?:()=>Promise<void>): Promise<void> {
  const promptExcerpt = (await input.innerText().catch(() => '')).trim().slice(0, 40);

  for (let i = 0; i < 8; i++) {
    if (verify) await verify();
    const sendBtn = iconButton(page, 'send').last();
    const isEnabled = await sendBtn.isEnabled().catch(() => false);

    if (isEnabled) {
      ctx.log.info('clicking send button', { attempt: i + 1 });
      await sendBtn.click().catch(() => undefined);
    } else {
      ctx.log.info('send button disabled or busy, attempting Enter on composer', { attempt: i + 1 });
      await input.click().catch(() => undefined);
      await sleep(150);
      await page.keyboard.press('Enter').catch(() => undefined);
    }

    // Wait 3.5s for Google to process the submission
    await sleep(3500);

    const afterClick = (await input.innerText().catch(() => '')).trim();
    const body = await bodyText(page);

    // If input cleared or the message is visible in chat history, it was accepted
    if (afterClick.length < 20 || (promptExcerpt && body.includes(promptExcerpt))) {
      ctx.log.info('generation request successfully accepted by Flow');
      return;
    }

    const hasApproval = await page.getByText(label('approve', 'exact')).first().isVisible().catch(() => false);
    if (hasApproval) {
      ctx.log.info('approval card appeared; generation started');
      return;
    }

    if (i % 2 === 1) {
      ctx.log.info('waiting for composer to accept request...', { attempt: i + 1 });
    }
  }

  throw new FlowError(
    'SEND_FAILED',
    'Flow composer did not accept prompt after multiple attempts.',
    {},
    await takeScreenshot(page, shots, 'send-failed'),
  );
}

function freshText(now: string, before: string): string {
  const tail = now.length > before.length ? now.slice(Math.max(0, before.length - 200)) : '';
  const diffLines = now
    .slice(-2000)
    .split('\n')
    .filter((line) => line.trim() && !before.includes(line))
    .join('\n');
  return `${tail}\n${diffLines}`;
}

async function failIfPolicyBlocked(ctx: AppContext, page: Page, bodyBefore: string): Promise<void> {
  const fresh = freshText(await bodyText(page), bodyBefore);
  const match = label('policyBlocked').exec(fresh);
  if (match) {
    const matchedLine =
      fresh
        .split('\n')
        .map((l) => l.trim())
        .find((l) => l && label('policyBlocked').test(l)) || match[0];
    throw new FlowError(
      'POLICY_BLOCKED',
      `Flow không tạo được video/ảnh: ${matchedLine}`,
      { detail: matchedLine },
      await takeScreenshot(page, path.join(ctx.config.stateDir, 'screenshots'), 'policy-blocked'),
    );
  }
}

// Card items are text nodes in the right-hand chat panel (below the top bar), not <button>s
async function findApproveButton(page: Page): Promise<Locator | null> {
  return firstInRegion(page, page.getByText(label('approve', 'exact')), 'right', 100);
}

async function awaitApproval(
  ctx: AppContext,
  page: Page,
  bodyBefore: string,
  kind: MediaKind,
  baselineTiles: number,
  timeoutMs: number,
  nudgeAllowed: boolean,
  baselineUrls?: Set<string>,
): Promise<string> {
  const shots = path.join(ctx.config.stateDir, 'screenshots');
  const t0 = Date.now();
  let nudges = 0;
  while (Date.now() - t0 < timeoutMs) {
    await sleep(1500);
    const approve = await findApproveButton(page);
    if (approve) {
      const approvalText = (await bodyText(page)).slice(-1200);
      await takeScreenshot(page, shots, 'approval-card');
      await approve.click();
      ctx.log.info('approval clicked', { card: /크레딧[^\n]*|credits?[^\n]*/i.exec(approvalText)?.[0] });
      return approvalText;
    }
    await failIfPolicyBlocked(ctx, page, bodyBefore);
    const fresh = freshText(await bodyText(page), bodyBefore);
    if (label('generating').test(fresh)) {
      ctx.log.info('generation already running (no approval card)');
      return '';
    }
    const currentTiles = (await mediaTiles(page)).filter((t) => t.kind === kind);
    if (
      currentTiles.length > baselineTiles ||
      (baselineUrls && currentTiles[0] && !baselineUrls.has(currentTiles[0].url))
    ) {
      ctx.log.info('new media tile appeared; generation started');
      return '';
    }
    // Only nudge once if the agent has not responded at all after 45s
    if (nudgeAllowed && nudges < 1 && !fresh.trim() && Date.now() - t0 > 45_000) {
      const input = composerInput(page);
      await humanTypePrompt(
        page,
        input,
        'Yes, proceed and generate it now, using the image I attached in my previous message as the reference.',
      ).catch(() => undefined);
      await sleep(150);
      await page.keyboard.press('Enter').catch(() => undefined);
      nudges++;
      ctx.log.info('nudged agent to proceed', { nudges });
    }
  }
  // With "confirm before generating: Never" there is usually no card at all; media polling decides
  return '';
}

// Flow's grid is newest-first and its tile addresses are re-signed over time, so outputs are identified by
// position: whatever grew the count of tiles of this kind since the baseline is what this job produced.
async function tileCount(page: Page, kind: MediaKind): Promise<number> {
  return (await mediaTiles(page)).filter((t) => t.kind === kind).length;
}

function allExistingPrefixes(outputDir: string): Set<string> {
  const out = downloadedPrefixes(outputDir);
  const parent = path.dirname(outputDir);
  try {
    for (const entry of fs.readdirSync(parent, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        for (const id of downloadedPrefixes(path.join(parent, entry.name))) {
          out.add(id);
        }
      }
    }
  } catch {
    // ignore
  }
  return out;
}

async function awaitOutputs(
  ctx: AppContext,
  page: Page,
  args: GenerateArgs,
  jobId: string,
  baselineTiles: number,
  bodyBefore: string,
  shots: string,
  baselineUrls?: Set<string>,
): Promise<{ path: string; media_id: string; title: string }[]> {
  fs.mkdirSync(args.outputDir, { recursive: true });
  const onDisk = allExistingPrefixes(args.outputDir);
  const tried = new Set<string>();
  const files: { path: string; media_id: string; title: string }[] = [];
  const t0 = Date.now();
  let lastShot = 0;
  let lastReload = Date.now();
  while (Date.now() - t0 < ctx.config.generationTimeoutMs && files.length < args.count) {
    await sleep(ctx.config.pollIntervalMs);
    await failIfPolicyBlocked(ctx, page, bodyBefore);
    // Flow's SPA does not push finished generations into the grid — polling the DOM alone shows the
    // baseline count forever, so every job timed out while the videos were in fact ready (byblos-book
    // 2026-09-05: 6 tiles reported for 30 min while the project grew from 5 to 8). Reload to pull them in.
    if (Date.now() - lastReload > 60_000) {
      await page.reload({ waitUntil: 'domcontentloaded' }).catch(() => undefined);
      await sleep(4000);
      const expand = await firstInRegion(
        page,
        page.locator('button:has-text("expand_content")'),
        'right',
      ).catch(() => null);
      if (expand) {
        await expand.click({ force: true }).catch(() => undefined);
        await sleep(800);
      }
      lastReload = Date.now();
    }
    const tiles = (await mediaTiles(page)).filter((t) => t.kind === args.kind);
    const grown = Math.max(0, tiles.length - baselineTiles);
    const candidateCount = baselineUrls
      ? Math.max(args.count + args.referenceImages.length + 2, grown)
      : grown;
    const fresh = tiles
      .slice(0, candidateCount)
      .filter((t) => !tried.has(t.url) && (!baselineUrls || !baselineUrls.has(t.url) || tiles.indexOf(t) < grown));
    if (fresh.length > 0) {
      await sleep(3000); // the thumbnail appears before encoding finishes
      for (const tile of fresh) {
        const attempt = await tryDownload(ctx.session.getContext(), tileUrl(tile), args.kind, args.outputDir, jobId);
        if (attempt.outcome !== 'ok') {
          ctx.log.warn('download retry later', { title: tile.title, reason: attempt.reason });
          continue;
        }
        tried.add(tile.url);
        const { media_id, path: file, bytes } = attempt.file;
        // A re-signed address makes the same clip look like a new tile; the digest is what says otherwise
        if (files.some((f) => f.media_id === media_id) || onDisk.has(media_id)) {
          try {
            fs.unlinkSync(file);
          } catch {
            // ignore
          }
          continue;
        }
        ctx.log.info('output downloaded', {
          id: media_id,
          path: file,
          bytes,
        });
        onDisk.add(media_id);
        files.push({ path: file, media_id, title: tile.title });
      }
    }
    const minute = Math.floor((Date.now() - t0) / 60_000);
    if (minute > lastShot) {
      lastShot = minute;
      await takeScreenshot(page, shots, `wait-${minute}m`);
    }
  }
  if (files.length === 0) {
    throw new FlowError(
      'GENERATION_TIMEOUT',
      `no ${args.kind}/* media appeared within ${Math.round(ctx.config.generationTimeoutMs / 1000)}s`,
      { baseline_tiles: baselineTiles, tiles_now: await tileCount(page, args.kind) },
      await takeScreenshot(page, shots, 'generation-timeout'),
    );
  }
  return files;
}

export function describeJob(job: Job): Record<string, unknown> {
  return { job_id: job.job_id, kind: job.kind, phase: job.phase, created_at: job.created_at, model: job.model };
}
