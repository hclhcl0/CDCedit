import test from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { attachCharacterIngredients, verifyCharacterIngredients } from './dist/flow/character-ingredients.js';

const character=name=>({id:`saved-${name}`,project_id:'project-fixture',name,voice:'kore',personality:'',portrait_media_id:'portrait-fixture',has_portrait:true});
const fixture=names=>`<main id="composer"><textarea></textarea><button aria-label="Add ingredients to the prompt box" id="attach">+</button><div id="chips"></div></main>
<section class="asset-item-container" hidden>${names.map(name=>`<button role="option" data-name="${name}">${name} Character</button>`).join('')}</section>
<button id="confirm" hidden>Add to prompt</button><button id="submit">Start generation</button>
<script>
window.submissions=0;
document.querySelector('#submit').onclick=()=>window.submissions++;
document.querySelector('#attach').onclick=()=>document.querySelector('section').hidden=false;
document.querySelectorAll('[role=option]').forEach(button=>button.onclick=()=>{
  document.querySelectorAll('[role=option]').forEach(other=>other.classList.remove('asset-item-active'));
  button.classList.add('asset-item-active');window.selectedName=button.dataset.name;
  document.querySelector('#confirm').hidden=false;
});
document.querySelector('#confirm').onclick=()=>{
  const chip=document.createElement('flow-ingredient-chip');
  const item=document.createElement('flow-character-ingredient-chip');item.textContent=window.selectedName;chip.append(item);
  document.querySelector('#chips').append(chip);document.querySelector('section').hidden=true;document.querySelector('#confirm').hidden=true;
};
</script>`;

test('saved character picker attaches both identities, rejects ambiguous/stale chips and never submits',async()=>{
  const browser=await chromium.launch({executablePath:'C:/Program Files/Google/Chrome/Application/chrome.exe',headless:true});
  try {
    const page=await browser.newPage();
    await page.setContent(fixture(['Nam','Lan']));
    const input=page.locator('textarea');
    await attachCharacterIngredients(page,input,[character('Nam'),character('Lan')]);
    assert.deepEqual(await page.locator('flow-character-ingredient-chip').allInnerTexts(),['Nam','Lan']);
    await verifyCharacterIngredients(page,input,[character('Nam'),character('Lan')]);
    assert.equal(await page.evaluate(()=>window.submissions),0);
    await assert.rejects(attachCharacterIngredients(page,input,[character('Nam')]),/Stale/);
    await page.locator('flow-character-ingredient-chip').last().evaluate(el=>el.remove());
    await assert.rejects(verifyCharacterIngredients(page,input,[character('Nam'),character('Lan')]),/verify/);
    await page.setContent(fixture(['Nam','Nam']));
    await assert.rejects(attachCharacterIngredients(page,page.locator('textarea'),[character('Nam')]),/uniquely/);
    assert.equal(await page.locator('flow-ingredient-chip').count(),0);
    assert.equal(await page.evaluate(()=>window.submissions),0);
  }finally{await browser.close();}
});
