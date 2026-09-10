/**
 * A broken page has to leave a record, and must never leave a blank window (ADR-0053).
 *
 * The failure these tests pin is not hypothetical: a render threw at the end of a
 * fifteen-minute turn, React unmounted the entire tree, and there was nothing on screen and
 * nothing in any log to say what had happened. Two rules come out of that, and both are
 * checked here rather than by eye:
 *
 *   1. every crash is recorded, exactly once, however it was thrown;
 *   2. every part of the app that can crash sits under a boundary, so the blast radius is a
 *      panel or a turn and never the window.
 */

import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { recentCrashes, reportCrash } from "../src/lib/crashReport.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (relative) => fs.readFileSync(path.join(here, "..", "src", relative), "utf8");

test("a crash is recorded whatever was thrown", () => {
  const fromError = reportCrash("render", new TypeError("Cannot read properties of null"), {
    surface: "the transcript",
    componentStack: "\n    at TurnRow\n    at Chat",
  });
  assert.equal(fromError.kind, "render");
  assert.equal(fromError.message, "Cannot read properties of null");
  assert.equal(fromError.surface, "the transcript");
  assert.ok(fromError.stack, "an Error carries its stack through");
  assert.match(fromError.componentStack, /TurnRow/);

  // Nothing says a thrown value is an Error. A reporter that assumes it is would itself
  // throw while reporting, which is the one thing it must never do.
  assert.equal(reportCrash("uncaught", "a bare string").message, "a bare string");
  assert.equal(reportCrash("rejection", { code: 42 }).message, '{"code":42}');
  assert.equal(reportCrash("uncaught", undefined).message, "undefined");
  assert.equal(reportCrash("uncaught", null).message, "null");

  // A cyclic object cannot be stringified; it still has to produce a line.
  const cyclic = {};
  cyclic.self = cyclic;
  assert.ok(reportCrash("rejection", cyclic).message.length > 0);
});

test("the history is bounded, because a render loop throws thousands of times", () => {
  for (let index = 0; index < 200; index += 1) {
    reportCrash("render", new Error(`boom ${index}`));
  }
  const kept = recentCrashes();
  assert.ok(kept.length <= 20, `kept ${kept.length}`);
  // Newest first: the one that just happened is the one being looked at.
  assert.equal(kept[0].message, "boom 199");
});

test("every crashable surface sits under a boundary", () => {
  // The root. Without this, React's own rule unmounts the whole tree on any render error —
  // which is precisely how the window went blank.
  const main = read("main.tsx");
  assert.match(main, /<ErrorBoundary/, "the app is wrapped");
  assert.match(main, /<CrashScreen/, "and a crash draws something rather than nothing");
  assert.match(main, /installCrashReporting\(\)/, "handlers are installed before render");
  assert.ok(
    main.indexOf("installCrashReporting()") < main.indexOf("createRoot"),
    "installed before anything can render, or the first crash is the one that is missed",
  );

  // The turn. This is the one that saves the work: a turn that cannot be drawn must not
  // take the conversation off the screen with it.
  const chat = read("screens/Chat.tsx");
  assert.match(chat, /<ErrorBoundary\s+key=\{turn\.id\}/, "each turn is its own boundary");
  assert.match(chat, /surface="a turn"/, "and the log says which surface broke");

  const boundary = read("components/ErrorBoundary.tsx");
  assert.match(boundary, /reportCrash\("render"/, "a caught render error is reported");
  assert.match(boundary, /componentStack/, "with the component that was rendering");
});

test("the crash screen says the work is safe, because it is", () => {
  // The conversation lives in Rust; only drawing it failed. Someone staring at a blank
  // window has no way to know that, and it is the difference between reloading and
  // assuming fifteen minutes are gone.
  const screen = read("components/CrashScreen.tsx");
  assert.match(screen, /safe/i);
  assert.match(screen, /Reload/);
  assert.match(screen, /logs/, "and points at where the details were written");
});
