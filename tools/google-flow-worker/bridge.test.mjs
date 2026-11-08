import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import {spawn} from 'node:child_process';

test('bridge stays alive, never generates without credit consent, recovers receipt files without a browser',{timeout:15000},async()=>{
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'concat-flow-bridge-'));
  const child=spawn(process.execPath,['bridge.mjs',dir],{stdio:['pipe','pipe','pipe'],windowsHide:true});
  const reader=readline.createInterface({input:child.stdout})[Symbol.asyncIterator]();
  const call=async(id,tool,args={},allowCredit=false)=>{child.stdin.write(JSON.stringify({id,tool,args,allowCredit})+'\n');const line=await reader.next();assert.equal(line.done,false);const result=JSON.parse(line.value);assert.equal(result.id,id);return result.result;};
  try{
    const denied=await call('denied','flow_generate_video',{auto_confirm:true});assert.equal(denied.ok,false);assert.match(denied.message,/authorization/);
    const deniedImage=await call('denied-image','flow_generate_image',{auto_confirm:true});assert.equal(deniedImage.ok,false);assert.match(deniedImage.message,/authorization/);
    const health=await call('health','health');assert.equal(health.ok,true);
    const video=path.join(dir,'test.mp4');fs.writeFileSync(video,'test');fs.writeFileSync(path.join(dir,'receipts','saved.json'),JSON.stringify({phase:'finished',updated:'2026-01-01',result:{files:[video]}}));fs.writeFileSync(path.join(dir,'receipts','uncertain.json'),JSON.stringify({phase:'uncertain',updated:'2026-01-02'}));
    const recent=await call('recent','recent');assert.deepEqual(recent.files,[video]);assert.equal(recent.unresolved,1);
    const invalid=await call('invalid','unknown');assert.equal(invalid.ok,false);assert.match(invalid.message,/Unsupported/);
  }finally{child.stdin.end();await new Promise(resolve=>child.once('exit',resolve));fs.rmSync(dir,{recursive:true,force:true});}
});
