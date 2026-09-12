import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { beginProjectDrag, projectDropAt } from "../src/chrome/projectPointerDrag.ts";

function fixture() {
  const dom = new JSDOM('<div class="proj-list"><div data-project-key="a"><button class="proj-head">A</button></div><div data-project-key="b"></div><div data-project-key="c"></div></div>');
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  const list = document.querySelector('.proj-list');
  const cards = [...list.children];
  list.getBoundingClientRect = () => ({ left: 0, right: 240, top: 0, bottom: 400 });
  cards.forEach((card, index) => { card.getBoundingClientRect = () => ({ top: 20 + index * 100, height: 80 }); });
  let order = ['a', 'b', 'c'];
  let target = null;
  let suppressed = false;
  const start = (index = 0, hit = cards[index]) => beginProjectDrag({ button: 0, pointerId: 1, clientX: 100, clientY: 40 + index * 100, currentTarget: cards[index], target: hit }, order[index],
    (_, value) => { target = value; }, (path, drop) => {
      order = order.filter(key => key !== path);
      order.splice(order.indexOf(drop.path) + (drop.position === 'after' ? 1 : 0), 0, path);
    }, () => { suppressed = true; });
  const pointer = (type, x, y, pointerId = 1) => window.dispatchEvent(Object.assign(new window.Event(type, { cancelable: true }), { clientX: x, clientY: y, pointerId }));
  return { dom, list, cards, start, pointer, order: () => order, target: () => target, suppressed: () => suppressed };
}

test('project drag commits on release and the remaining projects shift', () => {
  const f = fixture(); f.start(0, f.cards[0].firstChild);
  f.pointer('pointermove', 100, 350);
  assert.deepEqual(f.order(), ['a', 'b', 'c']);
  assert.deepEqual(f.target(), { path: 'c', position: 'after' });
  f.pointer('pointerup', 100, 350);
  assert.deepEqual(f.order(), ['b', 'c', 'a']);
  assert.equal(f.suppressed(), true);
  assert.equal(document.body.classList.contains('project-reordering'), false);
  assert.equal(f.target(), null);
  f.dom.window.close();
});

test('dragging upward uses the actual release position even without a final move', () => {
  const f = fixture(); f.start(2); f.pointer('pointermove', 100, 150);
  f.pointer('pointerup', 100, 25);
  assert.deepEqual(f.order(), ['c', 'a', 'b']); f.dom.window.close();
});

test('clicks, outside drops, Escape and pointer cancellation leave order intact', () => {
  for (const mode of ['click', 'outside', 'escape', 'cancel']) {
    const f = fixture(); f.start();
    if (mode !== 'click') f.pointer('pointermove', 100, 350);
    if (mode === 'escape') window.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }));
    if (mode === 'cancel') f.pointer('pointercancel', 100, 350);
    f.pointer('pointerup', mode === 'outside' ? 300 : 100, mode === 'click' ? 41 : 350);
    assert.deepEqual(f.order(), ['a', 'b', 'c'], mode); f.dom.window.close();
  }
});

test('action buttons do not start dragging and unrelated pointers cannot drop', () => {
  const f = fixture(); const button = document.createElement('button'); f.cards[0].append(button);
  f.start(0, button); f.pointer('pointermove', 100, 350); f.pointer('pointerup', 100, 350);
  assert.deepEqual(f.order(), ['a', 'b', 'c']);
  const cleanup = f.start(); f.pointer('pointermove', 100, 350); f.pointer('pointerup', 100, 350, 2);
  assert.deepEqual(f.order(), ['a', 'b', 'c']); cleanup(); f.dom.window.close();
});

test('drop hit testing handles top and bottom whitespace and rejects outside the list', () => {
  const f = fixture();
  assert.deepEqual(projectDropAt(f.list, 20, 0), { path: 'a', position: 'before' });
  assert.deepEqual(projectDropAt(f.list, 20, 390), { path: 'c', position: 'after' });
  assert.equal(projectDropAt(f.list, 250, 20), null); f.dom.window.close();
});
