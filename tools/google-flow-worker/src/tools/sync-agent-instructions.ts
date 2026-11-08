import { z } from 'zod';
import { syncProjectAgentInstructions } from '../flow/agent-instructions.js';
import { defineTool } from '../server/tool.js';

export const syncAgentInstructions = defineTool({
  name: 'flow_sync_agent_instructions',
  title: 'Sync AI-crafted guidelines into Flow Agent instructions',
  description:
    'Configures project-level Agent instructions in Google Flow: locks character visual identities with reference images, enforces slow cinematic camera pacing, and sets negative constraints for all future generations.',
  input: {
    project_url: z.string().optional(),
    instructions: z.array(
      z.object({
        title: z.string().describe('Title of the instruction (e.g. Character: Nam)'),
        text: z.string().describe('Guideline description for the agent'),
        image_name: z.string().optional().describe('Filename of reference image to attach to this instruction'),
      }),
    ),
  },
  output: {
    success: z.boolean(),
    synced_count: z.number(),
    message: z.string(),
  },
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true },
  async run(ctx, args) {
    const page = await ctx.session.ensureConnected();
    const res = await syncProjectAgentInstructions(ctx, page, {
      projectUrl: args.project_url,
      instructions: args.instructions.map((i) => ({
        title: i.title,
        text: i.text,
        imageName: i.image_name,
      })),
    });
    return {
      success: res.success,
      synced_count: res.syncedCount,
      message: res.message,
    };
  },
});
