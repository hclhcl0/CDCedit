import { z } from 'zod';
import { generateMedia } from '../flow/composer.js';
import { generateMediaDirect } from '../flow/direct-api.js';
import { VIDEO_MODEL_IDS } from '../flow/labels.js';
import { defineTool } from '../server/tool.js';

export const generateOutput = {
  status: z.enum(['ready_for_confirmation', 'completed']),
  job_id: z.string().describe('Pass back with resume=true to pick up this generation after a restart'),
  files: z.array(z.string()).describe('Absolute paths of downloaded outputs'),
  media_ids: z.array(z.string()).describe('One per file: the 8-char digest that names it'),
  references_attached: z.number(),
  characters_attached: z.number().optional().describe('Verified native saved-character ingredients, carrying their saved voices'),
  approval_text: z.string().describe('Text of the approval card if one was shown (contains the credit cost)'),
  model: z.string(),
  ratio: z.string(),
  duration: z.number().optional(),
  elapsed_ms: z.number(),
  prompt: z.string().describe('The exact instruction sent to the Flow agent'),
  screenshot: z.string().optional(),
};

export const generateVideo = defineTool({
  name: 'flow_generate_video',
  title: 'Generate a video',
  description:
    'Generate one video through the Flow agent composer or direct API and download it. Spends Flow credits when auto_confirm=true. ' +
    'Call with auto_confirm=false first: the prompt, settings and reference image are prepared and verified but nothing is sent. ' +
    'resume=true skips the composer and only waits for / downloads an in-flight generation (after a crash or timeout).',
  input: {
    prompt: z
      .string()
      .min(1)
      .describe('Scene description; the tool wraps it into an instruction with model/ratio/duration'),
    model: z.enum(VIDEO_MODEL_IDS).default('veo-3.1-fast'),
    ratio: z.enum(['9:16', '16:9']).default('9:16'),
    duration: z
      .number().int().min(3).max(15)
      .default(8)
      .describe('Seconds; Omni Flash 1.2: 3–15 (account limits apply); legacy Omni: 4/6/8/10; Veo: 4/6/8'),
    reference_images: z
      .array(z.string())
      .default([])
      .describe('Local image paths attached as the first frame / reference'),
    character_references: z.array(z.object({project_id:z.string(),id:z.string().regex(/^[A-Za-z0-9_-]{8,160}$/),voice:z.string(),local_name:z.string().max(2000).default('')})).max(8).default([])
      .describe('Saved Flow characters with their imported voice IDs. Attaches native character ingredients, not portrait uploads.'),
    project_url: z.url().describe('Project the video is generated in; outputs are saved to that project'),
    output_dir: z.string().describe('Local directory for the downloaded file (flow_<id8>_<job>.mp4)'),
    auto_confirm: z.boolean().default(false).describe('false = prepare only (0 credits); true = send and wait'),
    resume: z.boolean().default(false),
    job_id: z
      .string()
      .optional()
      .describe('Job to resume; defaults to the latest in-flight job for project_url + output_dir'),
    edit_mode: z
      .boolean()
      .default(false)
      .describe('Edit the last video of the current agent session instead of generating a new one'),
    use_direct_api: z
      .boolean()
      .default(true)
      .describe('Use direct in-browser API execution instead of DOM clicks'),
  },
  output: generateOutput,
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true },
  async run(ctx, args) {
    const valid = args.model === 'omni-flash-1.2' ? args.duration >= 3 && args.duration <= 15 : (args.model === 'omni-flash' ? [4,6,8,10] : [4,6,8]).includes(args.duration);
    if (!valid) throw new Error(`Duration ${args.duration}s is unsupported for ${args.model}`);

    const genArgs = {
      kind: 'video' as const,
      prompt: args.prompt,
      model: args.model,
      ratio: args.ratio,
      duration: args.duration,
      count: 1,
      referenceImages: args.reference_images,
      characterReferences: args.character_references,
      projectUrl: args.project_url,
      outputDir: args.output_dir,
      autoConfirm: args.auto_confirm,
      resume: args.resume,
      jobId: args.job_id,
      editMode: args.edit_mode,
    };

    if (args.use_direct_api && !args.resume && !args.edit_mode) {
      return generateMediaDirect(ctx, genArgs);
    }

    return generateMedia(ctx, genArgs);
  },
});
