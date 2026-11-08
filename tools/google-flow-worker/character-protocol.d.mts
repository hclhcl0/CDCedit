export interface FlowCharacter {
  id:string; project_id:string; name:string; voice:string; personality:string;
  portrait_media_id:string; has_portrait:boolean;
}
export function parseRpcEnvelope(raw:string,rpcId:string):unknown;
export function parseProjectCharacters(data:unknown,projectId:string):FlowCharacter[];
export function allowedImageUrl(value:unknown):string|undefined;
export function allowedDownloadUrl(value:string):string|undefined;
export function portraitUrl(data:unknown,mediaId:string,projectId:string):string;
export interface CharacterReference {project_id:string;id:string;voice:string;local_name?:string;}
export function resolveCharacterReferences(library:FlowCharacter[],requested:CharacterReference[],projectId:string):FlowCharacter[];
