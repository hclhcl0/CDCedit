import { z } from 'zod';
import { defineTool } from '../server/tool.js';
import { ensureOnProject } from '../flow/project.js';
import { listCharacters, downloadCharacter } from '../flow/characters.js';

const characterSchema=z.object({id:z.string(),project_id:z.string(),name:z.string(),voice:z.string(),personality:z.string(),portrait_media_id:z.string(),has_portrait:z.boolean()});

export const charactersList=defineTool({
  name:'flow_characters_list',title:'Read saved Flow characters',
  description:'Read names, personality notes, voice references and portrait identities from an existing project. Never generates or spends credits.',
  input:{project_url:z.url()},output:{project_id:z.string(),characters:z.array(characterSchema)},
  // Navigation shares the owned tab, so serialize it with other UI tasks.
  annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  async run(ctx,args){
    const page=await ctx.session.ensureConnected();
    const id=await ensureOnProject(page,args.project_url,ctx.log);
    return {project_id:id,characters:await listCharacters(page,id)};
  },
});

export const characterDownload=defineTool({
  name:'flow_character_download',title:'Download a saved character portrait and profile',
  description:'Re-read the selected project character, download its exact portrait, and return its profile. No generation or credit spending. Voice is metadata only, not an audio download.',
  input:{project_url:z.url(),character_id:z.string().regex(/^[A-Za-z0-9_-]{8,160}$/),output_dir:z.string()},
  output:{character:characterSchema,image_path:z.string(),warning:z.string()},
  annotations:{readOnlyHint:false,destructiveHint:false,idempotentHint:true,openWorldHint:true},
  async run(ctx,args){
    const page=await ctx.session.ensureConnected();
    const id=await ensureOnProject(page,args.project_url,ctx.log);
    return downloadCharacter(page,ctx.session.getContext(),id,args.character_id,args.output_dir);
  },
});
