import test from 'node:test';
import assert from 'node:assert/strict';
import {parseProjectCharacters,parseRpcEnvelope,portraitUrl,allowedImageUrl,allowedDownloadUrl,resolveCharacterReferences} from './character-protocol.mjs';

const project='11111111-1111-4111-8111-111111111111';
const other='22222222-2222-4222-8222-222222222222';
const character='33333333-3333-4333-8333-333333333333';
const workflow='44444444-4444-4444-8444-444444444444';
const media='55555555-5555-4555-8555-555555555555';
function library(){
  const row=[project,character,null,[1,'Minh',[[[workflow]],[[null,'kore']],'Thợ đèn lồng']],null];
  return [null,[[workflow,null,null,[null,null,null,null,media]]],[],[],[],[row]];
}

test('saved character reuse requires the same project, unique identity/name and unchanged voice',()=>{
  const items=parseProjectCharacters(library(),project);
  const ref={project_id:project,id:character,voice:'kore'};
  assert.equal(resolveCharacterReferences(items,[ref],project)[0].voice,'kore');
  assert.deepEqual(resolveCharacterReferences(items,[],project),[]);
  assert.throws(()=>resolveCharacterReferences(items,[ref],other),/project/);
  assert.throws(()=>resolveCharacterReferences(items,[ref,ref],project),/duplicated/);
  assert.throws(()=>resolveCharacterReferences(items,[{...ref,voice:'aoede'}],project),/voice changed/);
  assert.throws(()=>resolveCharacterReferences([],[ref],project),/missing/);
  assert.throws(()=>resolveCharacterReferences([{...items[0],has_portrait:false}],[ref],project),/portrait/);
  assert.throws(()=>resolveCharacterReferences([...items,{...items[0],id:workflow}],[ref],project),/ambiguous/);
  assert.throws(()=>resolveCharacterReferences([...items,{...items[0],id:workflow,name:'minh'}],[ref],project),/ambiguous/);
  assert.throws(()=>resolveCharacterReferences(items,Array(9).fill(ref),project),/At most 8/);
});

test('character import resolves the portrait through that character workflow, not gallery position',()=>{
  const result=parseProjectCharacters(library(),project);
  assert.deepEqual(result,[{id:character,project_id:project,name:'Minh',voice:'kore',personality:'Thợ đèn lồng',portrait_media_id:media,has_portrait:true}]);
  const missing=library();missing[1]=[];
  assert.equal(parseProjectCharacters(missing,project)[0].has_portrait,false);
  const foreign=library();foreign[5][0][0]=other;
  assert.throws(()=>parseProjectCharacters(foreign,project),/outside/);
  const duplicate=library();duplicate[5].push(duplicate[5][0]);
  assert.throws(()=>parseProjectCharacters(duplicate,project),/duplicate/);
});

test('RPC framing accepts only one matching read response and rejects error frames',()=>{
  const frame=['wrb.fr','Zzl0ze',JSON.stringify(library()),null,null,null];
  const raw=`)]}'\n123\n${JSON.stringify([frame])}\n`;
  assert.deepEqual(parseRpcEnvelope(raw,'Zzl0ze'),library());
  assert.throws(()=>parseRpcEnvelope(raw,'as29s'),/missing/);
  assert.throws(()=>parseRpcEnvelope(JSON.stringify([frame,frame]),'Zzl0ze'),/ambiguous/);
  assert.throws(()=>parseRpcEnvelope(JSON.stringify([['wrb.fr','Zzl0ze',null,null,null,7]]),'Zzl0ze'),/refused/);
});

test('portrait downloads verify media/project identities and constrain redirect hosts',()=>{
  const url='https://flow-content.google/image/reference-token?signature=fixture';
  assert.equal(portraitUrl([media,project,[url]],media,project),url);
  assert.throws(()=>portraitUrl([media,other,[url]],media,project),/identity/);
  assert.throws(()=>portraitUrl([character,project,[url]],media,project),/identity/);
  assert.equal(allowedImageUrl('https://flow-content.google.evil.example/image/a'),undefined);
  assert.equal(allowedImageUrl('https://flow-content.google/video/a'),undefined);
  assert.equal(allowedDownloadUrl('http://127.0.0.1/image/a'),undefined);
  assert.equal(allowedDownloadUrl('https://user:pass@storage.googleapis.com/a'),undefined);
  assert.equal(allowedDownloadUrl('https://storage.googleapis.com/bucket/image.png'),'https://storage.googleapis.com/bucket/image.png');
  assert.equal(allowedDownloadUrl('https://lh3.googleusercontent.com/image'),'https://lh3.googleusercontent.com/image');
});
