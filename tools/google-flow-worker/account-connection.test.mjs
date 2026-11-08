import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { once } from 'node:events';
import { defaultConfig } from './dist/config.js';
import { BrowserSession } from './dist/browser/session.js';
import { verifyManagedChrome, rememberManagedChrome } from './dist/browser/chrome.js';

test('managed CDP connection verifies account, profile and browser identity',async()=>{
  const root=await fs.mkdtemp(path.join(os.tmpdir(),'concat-owned-chrome-'));
  let browserId='browser-one';
  const server=http.createServer((req,res)=>{res.setHeader('content-type','application/json');res.end(JSON.stringify({webSocketDebuggerUrl:`ws://127.0.0.1:${server.address().port}/devtools/browser/${browserId}`}));});
  server.listen(0,'127.0.0.1');await once(server,'listening');
  const config={...defaultConfig(),connectionId:'first',cdpPort:server.address().port,stateDir:path.join(root,'worker'),userDataDir:path.join(root,'chrome')};
  try{
    await assert.rejects(verifyManagedChrome(config),/unverified/);
    await rememberManagedChrome(config);await verifyManagedChrome(config);
    await assert.rejects(verifyManagedChrome({...config,connectionId:'second'}),/different/);
    await assert.rejects(verifyManagedChrome({...config,profileDirectory:'Profile 2'}),/different/);
    browserId='another-browser';await assert.rejects(verifyManagedChrome(config),/different/);
  }finally{await new Promise(resolve=>server.close(resolve));await fs.rm(root,{recursive:true,force:true});}
});

test('attach-only never launches an unrelated profile when the port is closed',async()=>{
  const root=await fs.mkdtemp(path.join(os.tmpdir(),'concat-attach-only-'));
  const server=http.createServer();server.listen(0,'127.0.0.1');await once(server,'listening');const port=server.address().port;await new Promise(resolve=>server.close(resolve));
  const directory=path.join(root,'not-created');
  const config={...defaultConfig(),cdpPort:port,attachOnly:true,connectionId:'existing',chromePath:path.join(root,'missing-chrome.exe'),userDataDir:directory};
  try{
    const logger={info(){},warn(){},error(){},debug(){}};
    await assert.rejects(new BrowserSession(config,logger).ensureConnected(),/port is not open/);
    await assert.rejects(fs.stat(directory),{code:'ENOENT'});
  }finally{await fs.rm(root,{recursive:true,force:true});}
});
