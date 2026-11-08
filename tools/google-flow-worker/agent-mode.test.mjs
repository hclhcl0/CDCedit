import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { ensureAgentMode, openAgentSettings } from './dist/flow/agent-mode.js';

test('enters Agent once and opens accessible settings without submitting', async () => {
  const browser = await chromium.launch({ executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent(`<button aria-pressed="false" id="agent">Agent</button>
      <button aria-label="Settings trigger" id="standard"></button>
      <button id="instructions" hidden>Agent instructions</button>
      <button aria-label="Settings" id="settings" hidden></button>
      <button id="submit">Start generation</button>
      <script>
      window.counts = { toggle: 0, settings: 0, submit: 0 };
      agent.onclick = () => { counts.toggle++; agent.setAttribute('aria-pressed', 'true');
        standard.hidden = true; instructions.hidden = false; settings.hidden = false; };
      settings.onclick = () => counts.settings++;
      submit.onclick = () => counts.submit++;
      </script>`);
    await ensureAgentMode(page);
    await ensureAgentMode(page);
    await openAgentSettings(page);
    assert.deepEqual(await page.evaluate(() => window.counts), { toggle: 1, settings: 1, submit: 0 });
    await page.setContent('<button>Agent</button><button>Start generation</button>');
    await assert.rejects(ensureAgentMode(page), /Cannot determine Flow Agent mode safely/);
  } finally {
    await browser.close();
  }
});
