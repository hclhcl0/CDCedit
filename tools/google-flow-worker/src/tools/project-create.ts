import { z } from 'zod';
import { createNewProject } from '../flow/project.js';
import { defineTool } from '../server/tool.js';

export const projectCreate = defineTool({
  name: 'flow_project_create',
  title: 'Create a new Flow project',
  description: 'Navigate to Flow home, click "New project", and return the new project URL and ID.',
  input: {},
  output: {
    project_id: z.string(),
    project_url: z.string(),
  },
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true },
  async run(ctx) {
    const page = await ctx.session.ensureConnected();
    const result = await createNewProject(page, ctx.log);
    return result;
  },
});
