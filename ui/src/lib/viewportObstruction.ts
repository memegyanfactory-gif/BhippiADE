/**
 * Who is standing over the Studio viewport right now (SPA-001, ADR-0045).
 *
 * The Godot editor and the running game are *native* child windows placed over
 * `.godot-viewport`. The page cannot paint over them: a dropdown, a popover or a menu that
 * opens across that region is drawn underneath the engine and reads as "the button does
 * nothing". The only remedy the page has is the one the modals already use — tell Rust the
 * viewport is obstructed, and the native child hides for exactly that long.
 *
 * This module is the registry those floating surfaces join while they are open. It is
 * deliberately free of React so it can be tested directly; the hooks live in
 * `useViewportObstruction.ts`.
 */

type Listener = (count: number) => void;

const holders = new Set<string>();
const listeners = new Set<Listener>();
let nextId = 0;

function notify(): void {
  const count = holders.size;
  for (const listener of listeners) listener(count);
}

/** A fresh token for one floating surface. Stable for the life of that surface. */
export function newObstructionToken(): string {
  nextId += 1;
  return `obstruction-${nextId}`;
}

/** Declares that `token` is covering the viewport. Idempotent. */
export function obstructViewport(token: string): void {
  if (holders.has(token)) return;
  holders.add(token);
  notify();
}

/** Withdraws `token`. Idempotent — releasing twice is not an error. */
export function releaseViewport(token: string): void {
  if (!holders.delete(token)) return;
  notify();
}

/** How many surfaces are over the viewport. Zero means the native child may show. */
export function obstructionCount(): number {
  return holders.size;
}

export function isViewportObstructed(): boolean {
  return holders.size > 0;
}

/** Subscribes to count changes; returns the unsubscribe. */
export function subscribeObstruction(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Test seam: forgets every holder. Never called by product code. */
export function resetObstructionForTests(): void {
  holders.clear();
  notify();
}
