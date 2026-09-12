import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, readFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { JSDOM } from "jsdom";
import React, { act } from "react";
import { createRoot } from "react-dom/client";

test('picker selects detected IDs, keeps API and CLI separate, supports defaults/custom and does not activate its parent', async () => {
  const folder = await mkdtemp(join(process.cwd(), '.provider-picker-test-'));
  const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost', pretendToBeVisual: true });
  globalThis.window = dom.window; globalThis.document = dom.window.document;
  globalThis.localStorage = window.localStorage; globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  let root;
  try {
    const outfile = join(folder, 'picker.mjs');
    await build({ stdin: { contents: 'export { UnifiedModelPicker } from "./src/components/UnifiedModelPicker"; export { ChatUsageMeter } from "./src/components/ChatUsageMeter";', resolveDir: process.cwd(), loader: 'tsx' }, outfile, bundle: true, format: 'esm', platform: 'node', jsx: 'automatic', external: ['react', 'react/*', 'react-dom'], loader: { '.css': 'empty', '.png': 'dataurl' } });
    const { UnifiedModelPicker, ChatUsageMeter } = await import(pathToFileURL(outfile).href);
    root = createRoot(document.getElementById('root'));
    const selected = []; const settings = []; let parentClicks = 0;
    const providers = [
      { id: 'codex', label: 'Codex CLI', models: ['gpt-6-astra', 'gpt-5.6-sol'], accepts_custom_model: true },
      { id: 'openai', label: 'OpenAI API', models: ['api-only-model'] },
      { id: 'antigravity', label: 'Antigravity', models: ['gemini-3.8-flash-high'] },
      { id: 'local', label: 'Local server', models: [], accepts_custom_model: true },
    ];
    const render = async open => act(async () => root.render(React.createElement('div', { onClick() { parentClicks++; } },
      React.createElement(UnifiedModelPicker, { providers, currentProviderId: 'codex', currentModel: 'gpt-6-astra', open,
        onOpenChange() {}, onSelect: (...args) => selected.push(args), onOpenSettings: tab => settings.push(tab) }))));
    await render(true);
    const click = async selector => act(async () => { const el = document.querySelector(selector); assert.ok(el, selector); el.click(); });
    assert.equal(document.querySelector('[role="dialog"]').parentElement, document.body);
    assert.equal(document.querySelector('button[title="Claude"]'), null);
    await click('button[title="gpt-6-astra"]'); assert.deepEqual(selected.pop(), ['codex', 'gpt-6-astra']);
    await click('button[title="OpenAI API"]'); await click('button[title="api-only-model"]');
    assert.deepEqual(selected.pop(), ['openai', 'api-only-model']);
    await click('button[title="Antigravity"]'); await click('button[title="gemini-3.8-flash-high"]');
    assert.deepEqual(selected.pop(), ['antigravity', 'gemini-3.8-flash-high']);
    await click('button[title="Local server"]'); await click('.studio-model-list > button');
    assert.deepEqual(selected.pop(), ['local', null]);
    assert.ok(document.querySelector('input[aria-label="Custom model ID"]'));
    await click('button[title="Codex CLI"]');
    await act(async () => window.dispatchEvent(new window.KeyboardEvent('keydown', { key: '2', ctrlKey: true, bubbles: true })));
    assert.deepEqual(selected.pop(), ['codex', 'gpt-5.6-sol']);
    await click('button[title="Add to favorites"]'); await click('button[title="Favorites"]');
    assert.equal(document.querySelectorAll('.studio-model-card').length, 1);
    await click('.studio-model-main > .composer-bar-btn'); assert.deepEqual(settings, ['Providers']);
    assert.equal(parentClicks, 0);
    await render(false); assert.equal(document.querySelector('[role="dialog"]'), null);
    const usage = { id: 'codex', label: 'Codex CLI', models: [], total_tokens: 0, input_tokens: 0, output_tokens: 0,
      turns: 0, cost_usd: 0, limit_tokens: null, fraction: 0, spend_limit: null,
      account: { status: 'live', refreshed_at: new Date(2000).toISOString(), session: null,
        weekly: { used_fraction: 0.59, resets_at: 2000000000 }, note: 'Live account' } };
    const renderUsage = async (row, limits) => act(async () => root.render(React.createElement(ChatUsageMeter, {
      provider: providers[0], summary: { active_provider_id: 'codex', active: row, providers: [row] }, limits, open: true, onOpenChange() {},
    })));
    await renderUsage(usage, { provider: 'codex', at: 1000, snapshot: { weekly_used: 0.9 } });
    assert.match(document.querySelector('.ring-trigger').title, /41%/);
    await renderUsage({ ...usage, account: { ...usage.account, status: 'unavailable', weekly: null, note: 'Probe unavailable' } },
      { provider: 'codex', at: 1000, snapshot: { weekly_used: 0.9 } });
    assert.match(document.body.textContent, /No allowance reported/);
    assert.doesNotMatch(document.querySelector('.ring-trigger').title, /90%|10%|100%/);
    const app = await readFile('src/App.tsx', 'utf8');
    const openSession = app.slice(app.indexOf('const openSession ='), app.indexOf('const newCliSession ='));
    assert.ok(!openSession.includes('setWorkbenchOpen'), 'chat activation must not open the editor');
  } finally {
    if (root) await act(async () => root.unmount());
    dom.window.close(); await rm(folder, { recursive: true, force: true });
  }
});
