import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { JSDOM } from "jsdom";
import React, { act } from "react";
import { createRoot } from "react-dom/client";

test('Settings, About, account and updater show the release version, including loading states', async () => {
  const { version } = JSON.parse(await readFile('package.json', 'utf8'));
  const folder = await mkdtemp(join(process.cwd(), '.app-version-test-'));
  const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost', pretendToBeVisual: true });
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  globalThis.requestAnimationFrame = callback => window.requestAnimationFrame(callback);
  globalThis.cancelAnimationFrame = id => window.cancelAnimationFrame(id);
  let root;
  try {
    const outfile = join(folder, 'version.mjs');
    await build({ stdin: { contents: 'export { SettingsModal } from "./src/screens/SettingsModal"; export { SidebarAccount } from "./src/chrome/SidebarAccount"; export { AutoUpdateWidget } from "./src/chrome/AutoUpdateWidget";', resolveDir: process.cwd(), loader: 'tsx' }, outfile, bundle: true, format: 'esm', platform: 'node', jsx: 'automatic', external: ['react', 'react/*', 'react-dom'], loader: { '.css': 'empty', '.png': 'dataurl' }, plugins: [{ name: 'test-api', setup(build) {
      build.onResolve({ filter: /lib\/api$/ }, () => ({ path: 'api', namespace: 'test' }));
      build.onLoad({ filter: /.*/, namespace: 'test' }, () => ({ contents: 'export const api = {godotEngineCredit: async () => null}; export const events = {};' }));
    } }] });
    const { SettingsModal, SidebarAccount, AutoUpdateWidget } = await import(pathToFileURL(outfile).href);
    root = createRoot(document.getElementById('root'));
    for (const status of [null, { version }]) {
      await act(async () => root.render(React.createElement(SettingsModal, { status, initialTab: 'About', onClose() {}, onRefresh() {} })));
      assert.equal(document.querySelector('.rail-version').textContent, `bhippi v${version}`);
      assert.match(document.querySelector('.about-app-version').textContent, new RegExp(`^Version ${version.replaceAll('.', '\\.')}`));
    }
    await act(async () => root.render(React.createElement(SidebarAccount, { version, demoMode: true, collapsed: false, onOpenSettings() {} })));
    await act(async () => document.querySelector('.side-account-card').click());
    assert.equal(document.querySelector('.acct-card-version').textContent, `v${version}`);
    assert.equal(document.querySelector('.acct-card-version').title, `Version ${version}`);
    await act(async () => root.render(React.createElement(AutoUpdateWidget)));
    await act(async () => document.querySelector('.titlebar-update-btn').click());
    assert.equal(document.querySelector('.update-version-tag.current').textContent, `v${version}`);
    assert.doesNotMatch(document.body.textContent, /1\.1\.0|1\.1\.\d{12}/);
  } finally {
    if (root) await act(async () => root.unmount());
    dom.window.close();
    await rm(folder, { recursive: true, force: true });
  }
});
