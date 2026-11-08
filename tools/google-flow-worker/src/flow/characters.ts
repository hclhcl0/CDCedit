// Character reads adapted from RohaanA/google-flow-mcp (MIT), see ../../LICENSE.characters.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import type { APIResponse, BrowserContext, Page } from 'playwright-core';
import { allowedDownloadUrl, parseRpcEnvelope, parseProjectCharacters, portraitUrl, type FlowCharacter } from '../../character-protocol.mjs';
import { FlowError } from '../lib/errors.js';

/** Only the two non-generating read methods are exposed by this adapter. */
async function readRpc(page:Page, rpcId:'Zzl0ze'|'as29s', payload:unknown, projectId:string):Promise<unknown> {
  const response=await page.evaluate(async ({rpcId,payload,projectId})=>{
    if (location.origin!=='https://flow.google.com') throw Error('Open the signed-in Flow project first.');
    const wiz=(window as unknown as {WIZ_global_data?:Record<string,unknown>}).WIZ_global_data;
    const token=wiz?.['SNlM0e'];
    if (!wiz || typeof token!=='string') throw Error('Flow session is unavailable. Sign in manually in the Flow Chrome window.');
    const query=new URLSearchParams({rpcids:rpcId,'source-path':`/project/${projectId}`,hl:'en',rt:'c',
      bl:String(wiz['cfb2h']??''),'f.sid':String(wiz['FdrFJe']??''),_reqid:String(Date.now()%900000+100000)});
    const body=new URLSearchParams({'f.req':JSON.stringify([[[rpcId,JSON.stringify(payload),null,'generic']]]),at:token});
    // CSRF/session values stay inside the page. No credentials are returned to the worker.
    const result=await fetch(`/_/AiSandboxAngularFrontend/data/batchexecute?${query}`,{method:'POST',credentials:'include',
      headers:{'content-type':'application/x-www-form-urlencoded;charset=UTF-8','x-same-domain':'1'},body:body.toString()+'&',signal:AbortSignal.timeout(30000)});
    if (!result.ok) throw Error(`Flow read failed: HTTP ${result.status}`);
    const text=await result.text();
    if (text.length>12000000) throw Error('Flow read response is too large.');
    return text;
  },{rpcId,payload,projectId});
  return parseRpcEnvelope(response,rpcId);
}

export async function listCharacters(page:Page,projectId:string):Promise<FlowCharacter[]> {
  const data=await readRpc(page,'Zzl0ze',[`projects/${projectId}`,null,null,null,[1]],projectId);
  return parseProjectCharacters(data,projectId);
}

async function portraitResponse(context:BrowserContext,start:string):Promise<APIResponse>{
  let url=start;
  for(let i=0;i<6;i++){
    const response=await context.request.get(url,{maxRedirects:0,timeout:60000});
    if (![301,302,303,307,308].includes(response.status())) return response;
    const location=response.headers()['location'];
    await response.dispose();
    const next=location ? allowedDownloadUrl(new URL(location,url).href) : undefined;
    if (!next) throw new FlowError('DOWNLOAD_FAILED','Flow portrait redirected outside supported Google media hosts.');
    url=next;
  }
  throw new FlowError('DOWNLOAD_FAILED','Too many redirects downloading the character portrait.');
}

export async function downloadCharacter(page:Page,context:BrowserContext,projectId:string,characterId:string,outDir:string):Promise<{character:FlowCharacter;image_path:string;warning:string}> {
  const character=(await listCharacters(page,projectId)).find(item=>item.id===characterId);
  if (!character) throw new FlowError('UI_NOT_FOUND','Character no longer belongs to the selected Flow project. Refresh the library.');
  if (!character.portrait_media_id) return {character,image_path:'',warning:'Flow did not expose a portrait; only the profile was imported.'};
  const media=await readRpc(page,'as29s',[character.portrait_media_id],projectId);
  const url=portraitUrl(media,character.portrait_media_id,projectId);
  const response=await portraitResponse(context,url);
  try {
    if (!response.ok()) throw new FlowError('DOWNLOAD_FAILED',`Character portrait download failed: HTTP ${response.status()}`);
    const mime=(response.headers()['content-type']??'').split(';')[0]?.trim();
    const ext=mime==='image/png'?'png':mime==='image/jpeg'?'jpg':mime==='image/webp'?'webp':undefined;
    if (!ext) throw new FlowError('DOWNLOAD_FAILED','Character portrait is not PNG, JPEG or WebP.');
    const length=Number(response.headers()['content-length']??0);
    if (length>32*1024*1024) throw new FlowError('DOWNLOAD_FAILED','Character portrait exceeds 32 MB.');
    const bytes=await response.body();
    if (!bytes.length || bytes.length>32*1024*1024) throw new FlowError('DOWNLOAD_FAILED','Character portrait is empty or exceeds 32 MB.');
    const signature=ext==='png'?bytes.subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10]))
      :ext==='jpg'?bytes.subarray(0,3).equals(Buffer.from([255,216,255]))
      :bytes.subarray(0,4).toString()==='RIFF' && bytes.subarray(8,12).toString()==='WEBP';
    if (!signature) throw new FlowError('DOWNLOAD_FAILED','Character portrait has an invalid image signature.');
    await fs.mkdir(outDir,{recursive:true});
    const imagePath=path.join(outDir,`flow-character-${crypto.randomUUID()}.${ext}`);
    await fs.writeFile(imagePath,bytes,{flag:'wx'});
    return {character,image_path:imagePath,warning:''};
  } finally {await response.dispose();}
}
