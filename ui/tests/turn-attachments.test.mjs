/**
 * What a sent message attached, drawn back into the transcript.
 *
 * The owner, pasting a screenshot: *when i paste a screen shot or atach the image it shows me
 * like this* — over a bubble reading `Attached: pasted-20260911-050057.png (1.1 MB)`.
 *
 * The composer already showed a thumbnail. Pressing enter replaced it with a sentence about
 * a thumbnail, because a sent turn carried only `content`, and Rust appends that `Attached:`
 * line so the *model* knows what came with the message. Nothing stored the file, so nothing
 * could draw it: the picture was not lost, it was never asked for.
 *
 * These pin the repair — the turn keeps the paths, the pane asks for the bytes only when it
 * renders, and a file that has since gone still says what it was.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

const chat = read("src/screens/Chat.tsx");
const component = read("src/components/TurnAttachments.tsx");
const ipc = read("src/lib/ipc.ts");
const css = read("src/styles/chat.css");

// ── the turn remembers what was attached ────────────────────────────────────────

test("a stored turn carries the attachment paths, not just their names", () => {
  // Generated from Rust (INV-032), so this is proof the field survived code generation and
  // the pane is not reading something the backend never sends. A name cannot be turned back
  // into a picture; a path can.
  assert.match(
    ipc,
    /export type ChatTurnView = \{[\s\S]*?attachments\?: string\[\][\s\S]*?\n\}/,
    "ChatTurnView must carry attachments — re-export the bindings",
  );
});

test("the pane renders the pictures rather than the sentence about them", () => {
  assert.match(chat, /<TurnAttachments paths=\{turn\.attachments\} \/>/);
  assert.match(
    chat,
    /turn\.attachments && turn\.attachments\.length > 0/,
    "a turn with nothing attached must render exactly what it always did",
  );
});

test("the Attached: line is dropped from the bubble once the images are drawn", () => {
  // It stays in `content`, because that is what the model reads back in history. It just
  // stops being the caption of a missing image.
  assert.match(chat, /function userText\(turn: ChatTurnView\): string/);
  assert.match(chat, /if \(!turn\.attachments \|\| turn\.attachments\.length === 0\) return turn\.content;/);
  assert.match(
    chat,
    /replace\(\/\\n\\nAttached: \[\^\\n\]\*\$\/, ""\)/,
    "anchored at the end and single-line: exactly where attachment_trailer writes",
  );
});

test("a message with no attachments keeps its text byte for byte", () => {
  // Guards the strip from becoming a general rewrite of whatever the user typed. The early
  // return is the whole protection, so it is worth asserting it comes first.
  const fn = chat.slice(chat.indexOf("function userText"));
  const body = fn.slice(0, fn.indexOf("\n}"));
  const early = body.indexOf("return turn.content;");
  const strip = body.indexOf(".replace(");
  assert.ok(early > 0 && early < strip, "the untouched path must return before any replace");
});

// ── the bytes are fetched, once, and only when needed ───────────────────────────

test("previews are asked for on demand, not stored in the conversation", () => {
  // Twenty screenshots in a chat would be tens of megabytes of base64 in the transcript if
  // the turn carried bytes. It carries paths; this is where they become pictures.
  assert.match(component, /api\.attachmentPreview\(path\)/);
  assert.doesNotMatch(chat, /data_url.*turn\.attachments|turn\.attachments.*data_url/);
});

test("the same file referenced twice is read once", () => {
  assert.match(component, /const previewCache = new Map<string, AttachmentPreview \| null>\(\)/);
  assert.match(component, /const held = previewCache\.get\(path\);/);
  assert.match(component, /if \(held !== undefined\) return held;/);
});

test("a file that cannot be read is cached as a failure, not retried forever", () => {
  // `null` is a real answer here. Without it, reopening an old chat would re-hit the disk
  // for every dead temp file on every render.
  assert.match(component, /previewCache\.set\(path, null\);/);
});

test("an attachment that has gone still says what it was", () => {
  // A pasted image lives in the OS temp directory, so this is the normal end of one, not an
  // exotic failure. The turn is a record either way.
  assert.match(component, /no longer on disk/);
  assert.match(component, /function basename\(path: string\): string/);
  assert.match(
    component,
    /preview\?\.name \?\? basename\(path\)/,
    "the name comes from the path when there is no preview to take it from",
  );
});

test("the effect does not re-run on every render", () => {
  // `paths` is a fresh array each render, so a raw dependency would refetch forever.
  assert.match(component, /\}, \[paths\.join\("\|"\)\]\);/);
  assert.match(component, /let live = true;/, "and a late answer must not set state after unmount");
});

test("no stray control characters crept into the source", () => {
  // The first cut of this dependency key was a literal NUL, which compiled fine and turned
  // the file binary to every tool that reads it — grep included.
  // eslint-disable-next-line no-control-regex
  assert.doesNotMatch(component, /[\x00-\x08\x0b\x0c\x0e-\x1f]/);
});

// ── it is worth looking at ──────────────────────────────────────────────────────

/** One CSS rule body, so an assertion cannot wander past the closing brace into the next. */
function rule(selector) {
  const at = css.indexOf(`${selector} {`);
  assert.ok(at >= 0, `${selector} is not in chat.css`);
  return css.slice(at, css.indexOf("}", at));
}

test("an image in the transcript is bigger than the composer's receipt chip", () => {
  // 56px is right for "this is about to be sent". It is useless for "here is the screenshot
  // I am asking you about", which is what the owner was attaching.
  const img = rule(".turn-attachment-image img");
  assert.match(img, /max-height: 260px;/);
  assert.doesNotMatch(img, /width: 56px;/, "the transcript is not the composer");
  assert.match(img, /max-width: 100%;/, "and it never widens the bubble");
});

test("a click gives the image the width of the bubble, and says so", () => {
  assert.match(css, /\.turn-attachment-image\.expanded/);
  assert.match(css, /cursor: zoom-in;/);
  assert.match(css, /cursor: zoom-out;/);
  assert.match(component, /aria-expanded=\{expanded\}/);
  assert.match(component, /click to enlarge/);
});

test("the image is reachable and visible from the keyboard", () => {
  // It is a button, so it is already tabbable; the ring is what makes that usable.
  assert.match(component, /<button\s+type="button"\s+role="listitem"/);
  assert.match(css, /\.turn-attachment-image:focus-visible/);
});
