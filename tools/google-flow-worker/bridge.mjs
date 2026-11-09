// Local JSON-lines bridge used by the native Concat window. Upstream worker: MIT (LICENSE).
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';
import {fileURLToPath} from 'node:url';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StdioClientTransport} from '@modelcontextprotocol/sdk/client/stdio.js';

const root=path.dirname(fileURLToPath(import.meta.url));
const stateDir=process.argv[2];
const connection=process.argv[3] ? JSON.parse(process.argv[3]) : undefined;
if(connection && (!/^[\w-]{1,100}$/.test(connection.id??'') || !Number.isInteger(connection.cdpPort) || connection.cdpPort<1024 || connection.cdpPort>65535 || typeof connection.attachOnly!=='boolean' || !path.isAbsolute(connection.userDataDir??'') || typeof connection.profileDirectory!=='string' || /[\\/]/.test(connection.profileDirectory)))throw Error('Invalid account connection configuration');
if(!stateDir||!path.isAbsolute(stateDir))throw Error('Expected absolute state directory');
fs.mkdirSync(path.join(stateDir,'receipts'),{recursive:true});
let client;
async function connected(){if(!client){client=new Client({name:'concat-native-flow',version:'0.1.0'});await client.connect(new StdioClientTransport({command:process.execPath,args:[path.join(root,'dist/cli.js')],stderr:'inherit',env:{...process.env,FLOW_MCP_USER_DATA_DIR:connection?.userDataDir??path.join(stateDir,'chrome'),FLOW_MCP_PROFILE_DIRECTORY:connection?.profileDirectory??'Default',FLOW_MCP_STATE_DIR:path.join(stateDir,'worker'),FLOW_MCP_CDP_PORT:String(connection?.cdpPort??9337),FLOW_MCP_ATTACH_ONLY:String(connection?.attachOnly??false),FLOW_MCP_CONNECTION_ID:connection?.id??'',...(connection?.chromePath ? {FLOW_MCP_CHROME_PATH:connection.chromePath} : {})}}));}return client;}
const writeReceipt=(id,data)=>{const file=path.join(stateDir,'receipts',id+'.json');fs.writeFileSync(file+'.tmp',JSON.stringify(data,null,2));fs.renameSync(file+'.tmp',file);};
async function request(input){
  if(!/^[\w-]+$/.test(input.id??''))throw Error('Invalid request id');
  if(input.tool==='recent'){
    const receipts=fs.readdirSync(path.join(stateDir,'receipts')).filter(n=>n.endsWith('.json')).flatMap(name=>{try{return [JSON.parse(fs.readFileSync(path.join(stateDir,'receipts',name),'utf8'))];}catch{return [];}}).sort((a,b)=>b.updated.localeCompare(a.updated));
    return {ok:true,files:[...new Set(receipts.flatMap(r=>r.result?.files??[]))].filter(f=>fs.existsSync(f)),unresolved:receipts.filter(r=>['running','uncertain'].includes(r.phase)).length};
  }
  if(input.tool==='health'){await connected();return {ok:true,version:'native-flow-0.1'};}
  if(!input.tool.startsWith('flow_') && !input.tool.startsWith('vids_'))throw Error('Unsupported tool: ' + input.tool);
  const generating=['flow_generate_video','flow_generate_image','vids_generate_video'].includes(input.tool);
  const args=input.args??{};
  if(input.tool==='flow_character_download')args.output_dir=path.join(stateDir,'assets','flow-import',input.id);
  if(generating){args.output_dir=path.join(stateDir,'output',input.id);if(args.auto_confirm!==false&&input.allowCredit!==true)throw Error('Credit authorization required');}
  const receipt={id:input.id,phase:'running',tool:input.tool,updated:new Date().toISOString(),model:args.model};
  if(generating)writeReceipt(input.id,receipt);
  try{
    const c=await connected();const result=await c.callTool({name:input.tool,arguments:args},undefined,{timeout:1900000});
    const text=result.content?.filter(c=>c.type==='text').map(c=>c.text).join('\n')??'';
    let body;try{body=JSON.parse(text);}catch{body={ok:false,message:text};}
    if(result.isError)body.ok=false;
    if(generating)writeReceipt(input.id,{...receipt,phase:body.ok===false?'failed':'finished',result:body,updated:new Date().toISOString()});
    return body;
  }catch(error){if(generating)writeReceipt(input.id,{...receipt,phase:'uncertain',error:error.message,updated:new Date().toISOString()});throw error;}
}
process.stdout.on('error', () => {});
for await(const line of readline.createInterface({input:process.stdin,crlfDelay:Infinity})){
  let input;try{input=JSON.parse(line);const result=await request(input);process.stdout.write(JSON.stringify({id:input.id,result})+'\n');}
  catch(error){process.stdout.write(JSON.stringify({id:input?.id,result:{ok:false,message:error.message}})+'\n');}
}
if(client)await client.close();
