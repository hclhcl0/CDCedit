// Adapted from RohaanA/google-flow-mcp, MIT, a357dc36b8b9c2e8c85c809e4d80210149652fd0.
// Copyright (c) 2026 Muhammad Rohaan Atique. See LICENSE.characters.
const validId = value => typeof value === 'string' && /^[A-Za-z0-9_-]{8,160}$/.test(value);
const projectName = value => typeof value === 'string' ? value.replace(/^projects\//, '') : '';
const text = (value, limit) => typeof value === 'string' ? value.slice(0, limit) : '';

// Resolve by identity first; the UI picker only exposes names, so ambiguous names must fail.
export function resolveCharacterReferences(library, requested, projectId) {
  if (!Array.isArray(requested) || requested.length > 8) throw Error('At most 8 saved characters are supported.');
  const seen = new Set();
  return requested.map(ref => {
    if (!ref || ref.project_id !== projectId || !validId(ref.id) || seen.has(ref.id)) {
      throw Error('Saved character project/identity is invalid or duplicated. Nothing was sent.');
    }
    seen.add(ref.id);
    const matches = library.filter(item => item.project_id === projectId && item.id === ref.id);
    const item = matches[0];
    if (matches.length !== 1 || !item || !item.has_portrait) throw Error('Saved character is missing or has no portrait. Refresh the Flow library.');
    if (typeof ref.voice !== 'string' || item.voice !== ref.voice) throw Error('Saved character voice changed on Flow. Import it again before generating.');
    const name = item.name.trim().replace(/\s+/g, ' ');
    if (!name || library.filter(other => other.name.trim().replace(/\s+/g, ' ').toLowerCase() === name.toLowerCase()).length !== 1) {
      throw Error('Saved character name is ambiguous in the Flow picker. Rename it on Flow and refresh.');
    }
    return {...item, name};
  });
}

export function parseRpcEnvelope(raw, rpcId) {
  const frames = [];
  for (const line of raw.split('\n')) {
    if (!line.trim().startsWith('[')) continue;
    let rows;
    try { rows = JSON.parse(line); } catch { continue; }
    if (!Array.isArray(rows)) continue;
    frames.push(...rows.filter(row => Array.isArray(row) && row[0] === 'wrb.fr' && row[1] === rpcId));
  }
  if (frames.length !== 1) throw Error('Flow read response is missing or ambiguous; its protocol may have changed.');
  const frame = frames[0];
  if (frame[5] != null || typeof frame[2] !== 'string') throw Error('Flow refused the read request. Check login and project access.');
  try { return JSON.parse(frame[2]); } catch { throw Error('Flow returned an invalid read response.'); }
}

export function parseProjectCharacters(data, projectId) {
  if (!validId(projectId) || !Array.isArray(data) || (data[5] != null && !Array.isArray(data[5]))) {
    throw Error('Unrecognized Flow character library response.');
  }
  const workflows = new Map();
  for (const row of Array.isArray(data[1]) ? data[1] : []) {
    if (Array.isArray(row) && validId(row[0]) && validId(row[3]?.[4])) workflows.set(row[0], row[3][4]);
  }
  const seen = new Set();
  const result = [];
  for (const row of data[5] ?? []) {
    if (!Array.isArray(row) || !validId(row[1]) || projectName(row[0]) !== projectId) {
      throw Error('Flow returned a character outside the requested project or with an invalid identity.');
    }
    if (seen.has(row[1])) throw Error('Flow returned duplicate character identities; refusing to guess.');
    seen.add(row[1]);
    const info = Array.isArray(row[3]) ? row[3] : [];
    const character = Array.isArray(info[2]) ? info[2] : [];
    const references = Array.isArray(character[0]) ? character[0].map(item => item?.[0]).filter(validId) : [];
    const portrait = validId(row[4]) ? row[4] : references.map(id => workflows.get(id)).find(validId) ?? '';
    result.push({id:row[1],project_id:projectId,name:text(info[1],300) || row[1],
      voice:text(character[1]?.[0]?.[1],300),personality:text(character[2],20000),
      portrait_media_id:portrait,has_portrait:Boolean(portrait)});
  }
  if (result.length > 500) throw Error('Flow library exceeds the supported 500-character limit.');
  return result;
}

export function allowedImageUrl(value) {
  if (typeof value !== 'string') return undefined;
  try {
    const url = new URL(value.replace(/\\u003d/g,'=').replace(/\\u0026/g,'&'));
    if (url.protocol !== 'https:' || url.username || url.password || (url.port && url.port !== '443')) return undefined;
    const allowed = (url.hostname === 'flow-content.google' && url.pathname.startsWith('/image/'))
      || (url.hostname === 'flow.google.com' && url.pathname.startsWith('/asb/'));
    return allowed ? url.href : undefined;
  } catch { return undefined; }
}

export function allowedDownloadUrl(value) {
  const primary=allowedImageUrl(value);
  if (primary) return primary;
  try {
    const url=new URL(value);
    if (url.protocol!=='https:' || url.username || url.password || (url.port && url.port!=='443')) return undefined;
    return url.hostname==='storage.googleapis.com' || url.hostname.endsWith('.googleusercontent.com') ? url.href : undefined;
  } catch {return undefined;}
}

export function portraitUrl(data, mediaId, projectId) {
  if (!Array.isArray(data) || data[0] !== mediaId || projectName(data[1]) !== projectId) {
    throw Error('Portrait media identity does not match the selected character/project.');
  }
  const pending = [data];
  let visited = 0;
  while (pending.length) {
    if (++visited > 50000) throw Error('Flow media metadata is too large.');
    const value = pending.pop();
    if (typeof value === 'string') { const url = allowedImageUrl(value); if (url) return url; }
    else if (Array.isArray(value)) pending.push(...value);
    else if (value && typeof value === 'object') pending.push(...Object.values(value));
  }
  throw Error('Flow did not expose a supported portrait URL; no other image was substituted.');
}
