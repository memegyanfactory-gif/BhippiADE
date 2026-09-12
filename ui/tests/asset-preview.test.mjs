import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { JSDOM } from "jsdom";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { createTabsState, openTab } from "../src/workbench/editorTabs.ts";

test('selected assets render as images or model surfaces, with source toggle and viewer cleanup', async () => {
  const folder = await mkdtemp(join(process.cwd(), '.asset-preview-test-'));
  const calls = [];
  globalThis.__previewApi = {
    assetPreviewOpen: async (...args) => { calls.push(['open', ...args]); },
    assetPreviewClose: async (...args) => { calls.push(['close', ...args]); },
    assetPreviewLayout: async (...args) => { calls.push(['layout', ...args]); },
    assetPreviewStatus: async () => null,
  };
  const dom = new JSDOM('<div id="root"></div>', { pretendToBeVisual: true });
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  let frames = new Map(); let nextFrame = 0;
  globalThis.requestAnimationFrame = callback => { frames.set(++nextFrame, callback); return nextFrame; };
  globalThis.cancelAnimationFrame = id => frames.delete(id);
  dom.window.HTMLElement.prototype.getBoundingClientRect = () => ({ left: 10, top: 20, width: 640, height: 480 });
  let root;
  try {
    const outfile = join(folder, 'preview.mjs');
    await build({ entryPoints: ['src/workbench/CodeView.tsx'], outfile, bundle: true, format: 'esm', platform: 'node', jsx: 'automatic', external: ['react', 'react/*'], loader: { '.css': 'empty' }, plugins: [{ name: 'test-api', setup(build) {
      build.onResolve({ filter: /lib\/api$/ }, () => ({ path: 'test-api', namespace: 'test' }));
      build.onLoad({ filter: /.*/, namespace: 'test' }, () => ({ contents: 'export const api = globalThis.__previewApi;' }));
    } }] });
    const { CodeView } = await import(pathToFileURL(outfile).href);
    root = createRoot(document.getElementById('root'));
    const file = changes => openTab(createTabsState(), { path: 'icon.svg', name: 'icon.svg', text: '<svg xmlns="http://www.w3.org/2000/svg"><circle r="10"/></svg>', language: 'svg', bytes: 100, editable: true, truncated: false, preview_kind: 'image', preview_mime: 'image/svg+xml', indentStyle: { useTabs: false, size: 2 }, eol: 'LF', ...changes }).state.tabs[0];
    const props = tab => ({ tabs: [tab], activeTab: tab, projectPath: 'C:/project', visible: true, onChange() {}, onSave() {}, onCloseTab() {}, onSwitchTab() {}, onPinTab() {}, onReorderTab() {}, onUndo() {}, onRedo() {} });
    const svg = file({});
    await act(async () => root.render(React.createElement(CodeView, props(svg))));
    assert.match(document.querySelector('img').src, /^blob:/);
    assert.equal(document.querySelector('textarea'), null);
    await act(async () => [...document.querySelectorAll('button')].find(button => button.textContent === 'Source').click());
    assert.equal(document.querySelector('textarea').value, svg.text);
    assert.equal(document.querySelector('textarea').readOnly, false);
    await act(async () => [...document.querySelectorAll('button')].find(button => button.textContent === 'Preview').click());
    assert.ok(document.querySelector('img'));
    const png = file({ path: 'large.png', name: 'large.png', text: '', language: 'png', bytes: 2_000_000, editable: false, preview_mime: 'image/png', content_base64: 'iVBORw0KGgo=' });
    await act(async () => root.render(React.createElement(CodeView, props(png))));
    assert.match(document.querySelector('img').src, /^data:image\/png;base64,/);
    const model = file({ path: 'mesh.glb', name: 'mesh.glb', text: '', language: 'glb', editable: false, preview_kind: 'model', preview_mime: null });
    await act(async () => root.render(React.createElement(CodeView, props(model))));
    assert.equal(document.querySelector('textarea'), null);
    assert.ok(document.querySelector('.model-preview-surface'));
    assert.equal(calls.filter(call => call[0] === 'open').length, 1);
    assert.deepEqual(calls.find(call => call[0] === 'open').slice(2), ['C:/project', 'mesh.glb']);
    const flushFrame = async () => { const pending = frames; frames = new Map(); await act(async () => { for (const callback of pending.values()) callback(performance.now()); }); };
    await flushFrame();
    assert.equal(calls.filter(call => call[0] === 'layout').at(-1)[3], true);
    await act(async () => root.render(React.createElement(CodeView, { ...props(model), visible: false })));
    await flushFrame();
    assert.equal(calls.filter(call => call[0] === 'layout').at(-1)[3], false);
    await act(async () => root.render(React.createElement(CodeView, props(png))));
    assert.equal(calls.filter(call => call[0] === 'close').length, 1);
    assert.equal(frames.size, 0);
    const binary = file({ path: 'data.bin', name: 'data.bin', text: '', editable: false, preview_kind: 'binary', preview_mime: null });
    await act(async () => root.render(React.createElement(CodeView, props(binary))));
    assert.match(document.querySelector('.asset-error').textContent, /binary file has no preview/);
    assert.equal(document.querySelector('textarea'), null);
  } finally {
    if (root) await act(async () => root.unmount());
    dom.window.close();
    delete globalThis.__previewApi;
    await rm(folder, { recursive: true, force: true });
  }
});
