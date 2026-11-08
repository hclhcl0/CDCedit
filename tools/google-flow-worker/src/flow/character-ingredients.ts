// UI ingredient selection adapted from RohaanA/google-flow-mcp, MIT (LICENSE.characters).
import type { Locator, Page } from 'playwright-core';
import type { FlowCharacter } from '../../character-protocol.mjs';
import { FlowError } from '../lib/errors.js';
import { labelButton, namedButton, sleep } from './ui.js';

const escape = (text:string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Scope verification to the composer that owns our input, never the project gallery. */
async function composer(page:Page,input:Locator):Promise<Locator> {
  const found=await input.evaluate(editor=>{
    document.querySelectorAll('[data-concat-character-composer]').forEach(el=>el.removeAttribute('data-concat-character-composer'));
    let el:Element|null=editor;
    while(el && !el.querySelector('button[aria-label*="prompt box"], button[aria-label*="프롬프트 상자"]')) el=el.parentElement;
    if (!el || el===document.body || el===document.documentElement) return false;
    el.setAttribute('data-concat-character-composer','1');
    return true;
  });
  if (!found) throw new FlowError('REFERENCE_NOT_ATTACHED','Cannot identify the Flow character composer. Nothing was sent.');
  return page.locator('[data-concat-character-composer]');
}

export async function verifyCharacterIngredients(page:Page,input:Locator,characters:FlowCharacter[]):Promise<void> {
  const root=await composer(page,input);
  const chips=root.locator('flow-ingredient-chip').filter({has:page.locator('flow-character-ingredient-chip')});
  const names=await chips.allInnerTexts();
  if (names.length!==characters.length || characters.some(item=>names.filter(name=>new RegExp(`(?:^|\\s)${escape(item.name)}(?:$|\\s)`).test(name.trim().replace(/\s+/g,' '))).length!==1)) {
    throw new FlowError('REFERENCE_NOT_ATTACHED','Could not verify every saved character ingredient. Nothing was sent.');
  }
}

export async function attachCharacterIngredients(page:Page,input:Locator,characters:FlowCharacter[]):Promise<void> {
  const root=await composer(page,input);
  if (await root.locator('flow-character-ingredient-chip').count()) throw new FlowError('REFERENCE_NOT_ATTACHED','Stale character ingredients remain in the composer. Clear the prompt and retry.');
  for (const item of characters) {
    const before=await root.locator('flow-ingredient-chip').count();
    await namedButton(page,'attachToPrompt').first().click();
    const options=page.locator('.asset-item-container button[role="option"]:visible');
    await options.first().waitFor({state:'visible',timeout:10000});
    const search=page.getByPlaceholder('Search assets',{exact:true});
    if (await search.isVisible()) {await search.fill(item.name);await sleep(800);}
    // Both exact name and Character type are required; a portrait Image is insufficient.
    const candidate=options.filter({hasText:new RegExp(`^\\s*${escape(item.name)}\\s+(?:Character|캐릭터)\\b`,'i')});
    if (await candidate.count()!==1) throw new FlowError('REFERENCE_NOT_ATTACHED',`Cannot uniquely select saved character ${item.name}. Nothing was sent.`);
    await candidate.click();
    await page.waitForFunction(el=>Boolean(el && el.classList.contains('asset-item-active')),await candidate.elementHandle(),{timeout:5000});
    await labelButton(page,'addToPrompt','exact').first().click();
    await sleep(500);
    if (await root.locator('flow-ingredient-chip').count()!==before+1) throw new FlowError('REFERENCE_NOT_ATTACHED','Flow did not attach the saved character ingredient. Nothing was sent.');
  }
  await verifyCharacterIngredients(page,input,characters);
}
