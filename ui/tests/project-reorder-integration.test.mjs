import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { JSDOM } from "jsdom";
import React, { act } from "react";
import { createRoot } from "react-dom/client";

test('real sidebar drops reorder neighboring cards, persist, and cross the pinned boundary', async () => {
  const folder = await mkdtemp(join(process.cwd(), '.sidebar-drag-test-'));
  const dom = new JSDOM('<div id="root"></div>', { url: 'http://localhost', pretendToBeVisual: true });
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  globalThis.requestAnimationFrame = callback => window.requestAnimationFrame(callback);
  globalThis.cancelAnimationFrame = id => window.cancelAnimationFrame(id);
  let root;
  try {
    const outfile = join(folder, 'sidebar.mjs');
    await build({ entryPoints: ['src/chrome/Sidebar.tsx'], outfile, bundle: true, platform: 'node', format: 'esm', jsx: 'automatic', external: ['react', 'react/*', 'react-dom'], loader: { '.png': 'dataurl', '.css': 'empty' }, plugins: [{ name: 'test-api', setup(build) {
      build.onResolve({ filter: /lib\/api$/ }, () => ({ path: 'api', namespace: 'test' }));
      build.onLoad({ filter: /.*/, namespace: 'test' }, () => ({ contents: 'export const api = {status: async () => ({version: "test"}), projectTools: async () => []};' }));
    } }] });
    const { Sidebar } = await import(pathToFileURL(outfile).href);
    const projects = ['alpha', 'beta', 'gamma'].map(name => ({ name, path: `c:/${name}`, branch: 'main' }));
    const noop = () => {};
    const props = { screen: 'projects', projects, project: projects[0], sessions: [], sessionsError: null, activeConversationId: null, collapsed: false, demoMode: true, onScreen: noop, onBack: noop, onForward: noop, onToggle: noop, onDeleteConversation: noop, onOpenSession: noop, onNewSessionInProject: noop, onRemoveProject: noop, onSelectProject: noop, onNewProject: noop, onRetrySessions: noop };
    root = createRoot(document.getElementById('root'));
    await act(async () => root.render(React.createElement(Sidebar, props)));
    const keys = () => [...document.querySelectorAll('[data-project-key]')].map(card => card.dataset.projectKey);
    const move = async (from, y) => {
      const list = document.querySelector('.proj-list');
      list.getBoundingClientRect = () => ({ left: 0, right: 240, top: 0, bottom: 400 });
      [...list.querySelectorAll('[data-project-key]')].forEach((card, index) => {
        card.getBoundingClientRect = () => ({ top: 20 + index * 100, height: 80 });
      });
      const head = list.querySelector(`[data-project-key="${from}"] .proj-head`);
      const pointer = (target, type, clientY) => target.dispatchEvent(Object.assign(new window.Event(type, { bubbles: true, cancelable: true }), { button: 0, pointerId: 1, clientX: 100, clientY }));
      await act(async () => pointer(head, 'pointerdown', 40 + keys().indexOf(from) * 100));
      await act(async () => pointer(window, 'pointermove', y));
      await act(async () => pointer(window, 'pointerup', y));
    };
    assert.deepEqual(keys(), ['c:/alpha', 'c:/beta', 'c:/gamma']);
    await move('c:/alpha', 350);
    assert.deepEqual(keys(), ['c:/beta', 'c:/gamma', 'c:/alpha']);
    assert.deepEqual(JSON.parse(window.localStorage.getItem('bhippi-project-order')), keys());
    await act(async () => root.unmount());
    root = createRoot(document.getElementById('root'));
    window.localStorage.setItem('bhippi-project-pins', JSON.stringify(['c:/beta']));
    await act(async () => root.render(React.createElement(Sidebar, props)));
    assert.deepEqual(keys(), ['c:/beta', 'c:/gamma', 'c:/alpha']);
    await move('c:/alpha', 25);
    assert.deepEqual(keys(), ['c:/alpha', 'c:/beta', 'c:/gamma']);
    assert.deepEqual(new Set(JSON.parse(window.localStorage.getItem('bhippi-project-pins'))), new Set(['c:/alpha', 'c:/beta']));
  } finally {
    if (root) await act(async () => root.unmount());
    dom.window.close();
    await rm(folder, { recursive: true, force: true });
  }
});
