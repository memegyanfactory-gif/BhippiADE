/**
 * The Studio chat column and the chat bar it carries.
 *
 * These are source and stylesheet facts rather than behaviour, and that is deliberate:
 * the bug this locks down was pure layout. `.composer-zone` was `height: 100%` inside the
 * studio dock, so the transcript had no room — messages were sent and answered but never
 * appeared, and because the same block also hid `.error-inline`, a failed send showed
 * nothing at all. Neither is visible to a unit test of the chat logic, and both come back
 * the moment someone "tidies" the studio override block, so they are asserted here.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");

const chat = read("../src/screens/Chat.tsx");
const chatCss = read("../src/styles/chat.css");
const studioCss = read("../src/styles/studio.css");
const popovers = read("../src/components/ComposerPopovers.tsx");
const apiTs = read("../src/lib/api.ts");
const ipcTs = read("../src/lib/ipc.ts");

/**
 * Every top-level declaration block written for exactly this selector, in source
 * order. The leading newline anchors the match to column zero, so a same-selector
 * override nested inside an `@container` or `@media` block is not mistaken for
 * the rule that applies everywhere.
 */
function rulesFor(css, selector) {
  const needle = `\n${selector} {`;
  const bodies = [];
  let at = css.indexOf(needle);
  while (at >= 0) {
    const end = css.indexOf("}", at);
    bodies.push(css.slice(at + needle.length, end));
    at = css.indexOf(needle, end);
  }
  return bodies;
}

/** The last block wins in the cascade, so that is the one worth asserting on. */
function lastRule(css, selector) {
  const bodies = rulesFor(css, selector);
  assert.ok(bodies.length > 0, `no rule for ${selector}`);
  return bodies[bodies.length - 1];
}

const studioSel = (suffix) => `.studio-main-layout .studio-left-column ${suffix}`;

// ── The studio column is a real column ────────────────────────────────────────

test("the studio transcript takes the column and the composer keeps its own height", () => {
  const threadWrap = lastRule(studioCss, studioSel(".thread-wrap"));
  assert.match(threadWrap, /flex:\s*1/, "the transcript takes the leftover height");
  assert.match(threadWrap, /min-height:\s*0/, "…and may shrink below its content");
  // The inner .thread is the scroller. If the wrapper scrolled too, the composer
  // would scroll off the bottom of the dock instead of staying pinned to it.
  assert.match(threadWrap, /overflow:\s*hidden/);

  const composerZone = lastRule(studioCss, studioSel(".composer-zone"));
  assert.doesNotMatch(
    composerZone,
    /height:\s*100%/,
    "a full-height composer leaves the transcript no room — this was the bug",
  );
  assert.match(composerZone, /flex:\s*none/, "the composer is sized by its content");

  // The chat itself still fills the dock, or the column collapses to its content.
  assert.match(lastRule(studioCss, studioSel(".chat")), /height:\s*100%/);
});

test("the studio column never hides an error or a queued message", () => {
  // One rule group in studio.css hides what the narrow dock cannot carry. Whatever
  // else is on that list, a failed send and an unsent queued message are not.
  const hidden = studioCss
    .split(/\n\s*\n/)
    .filter((block) => /display:\s*none/.test(block) && block.includes(".studio-left-column"))
    .join("\n");
  assert.ok(hidden.length > 0, "the studio override block should still exist");
  assert.ok(
    !hidden.includes(".error-inline"),
    "a send that failed must say so in the studio, not fail silently",
  );
  assert.ok(
    !hidden.includes(".queued-messages-wrap"),
    "a queued message the user cannot see is a message they send twice",
  );
  // The empty conversation still shows its welcome rather than an empty column.
  assert.ok(!hidden.includes(".thread-inner-empty"));
  assert.ok(!hidden.includes(".chat-welcome"));
});

test("the empty studio conversation renders the welcome", () => {
  assert.match(
    chat,
    /turns\.length === 0 \? \(\s*<div className="thread-inner thread-inner-empty">\s*<ChatWelcome/,
  );
});

test("the studio dock widens the chat's own centred maxima to the column", () => {
  // .composer-shell, .composer-bar, .error-inline and the queue all cap themselves at
  // --chat-content-max for the full-width chat; in a 320px dock that cap has to go or
  // they sit in a narrow strip inside an already narrow column.
  const widened = studioCss.slice(
    studioCss.indexOf(studioSel(".error-inline")),
    studioCss.indexOf(studioSel(".composer textarea")),
  );
  for (const part of [".error-inline", ".queued-messages-wrap", ".composer-shell", ".composer-bar"]) {
    assert.ok(widened.includes(part), `${part} should be widened to the dock`);
  }
  assert.match(widened, /max-width:\s*none/);
});

// ── The chat bar shape ────────────────────────────────────────────────────────

test("the composer placeholder tells you how to reach the commands", () => {
  assert.ok(chat.includes('"Type / for commands"'));
  assert.ok(!chat.includes('"Ask anything"'), "the old placeholder is gone, not shadowed");
});

test("the control strip is below the input box, outside it", () => {
  const barAt = chat.indexOf('<div className="composer-bar">');
  assert.ok(barAt > 0, "the composer bar should exist");
  const shellAt = chat.indexOf("composer-shell${streaming");
  assert.ok(shellAt > 0 && shellAt < barAt, "the bar comes after the box");

  // Between the last send button and the strip, four elements close: the action
  // group, the input, the composer and the shell. If the strip were still inside
  // the box there would be fewer.
  const gap = chat.slice(chat.lastIndexOf("composer-circle-send", barAt), barAt);
  assert.equal(
    (gap.match(/<\/div>/g) ?? []).length,
    4,
    "the input box must be closed before the control strip opens",
  );
});

test("the strip has a left group and a right group, and every control kept its place", () => {
  const leftAt = chat.indexOf('<div className="composer-bar-left">');
  const rightAt = chat.indexOf('<div className="composer-bar-right">');
  assert.ok(leftAt > 0 && rightAt > leftAt, "left group, then right group");

  const left = chat.slice(leftAt, rightAt);
  assert.ok(left.includes("<PermissionPopover"), "the mode chip is on the left");
  assert.ok(left.includes("<OptionsPopover"), "the add/insert menu is on the left");
  assert.ok(left.includes("tool-btn mic"), "the mic is on the left");

  const right = chat.slice(rightAt);
  // The perception dot left the strip in SPA-002: the right group reads model · effort ·
  // ring, as the reference bar does, and the desktop toggle lives in the permission popover.
  for (const control of [
    "<ProviderPopover",
    "<ModelPopover",
    "<ThinkingPopover",
    "<ChatUsageMeter",
  ]) {
    assert.ok(right.includes(control), `${control} belongs to the right group`);
  }
  assert.ok(!right.includes("dot-trigger"), "the perception dot is not in the strip");
  // A re-arrangement, not a feature change: nothing was dropped on the way.
  assert.ok(!left.includes("<ModelPopover"));
});

test("Quick / Balanced / Max is gone from the composer strip", () => {
  // The owner's complaint was the strip itself: provider, model and effort already
  // say everything a tier said, and a fourth boxed control beside three plain ones is
  // what made the bar look bad. The component and lib/tiers.ts stay — Settings ›
  // Providers still edits the presets — but nothing in the chat bar renders them.
  assert.ok(!chat.includes("<TierChips"), "no tier chips in the composer");
  assert.ok(!chat.includes("TierChips"), "and the import went with them");
  assert.ok(!chat.includes("applyTier"), "the preset writer had one caller and left with it");
});

test("the send button appears only with something to send or a turn to stop", () => {
  assert.match(
    chat,
    /\) : hasDraft \? \(\s*<button\s*type="button"\s*className="composer-circle-send"/,
    "no permanently disabled circle sitting in the box",
  );
  // An attached photo with nothing typed is still a message, so "something to send"
  // is the draft, not the textarea.
  assert.match(
    chat,
    /const hasDraft = input\.trim\(\)\.length > 0 \|\| attachments\.length > 0;/,
  );
  assert.match(chat, /className="composer-circle-send stop"/, "the stop variant while running");
  assert.match(chat, /className="composer-circle-send queue"/);
});

test("the attach glyph lives inside the box and opens the same picker the + menu does", () => {
  const actionsAt = chat.indexOf('<div className="composer-action-group">');
  const barAt = chat.indexOf('<div className="composer-bar">');
  const attachAt = chat.indexOf('className="composer-attach-btn"');
  assert.ok(attachAt > actionsAt && attachAt < barAt, "the glyph is inside the input box");
  assert.match(lastRule(chatCss, ".composer-attach-btn"), /width:\s*26px/);

  // One attachment path, not two: the glyph and the menu row call the same thing, and
  // neither of them types an `@` any more — that is the workspace-mention autocomplete,
  // which is a different feature and stays.
  const glyph = chat.slice(attachAt, attachAt + 400);
  assert.match(glyph, /onClick=\{\(\) => void pickAttachments\(\)\}/);
  assert.ok(!glyph.includes('`${prev} @`'), "the glyph no longer fakes an attachment");
  assert.match(chat, /onAttach=\{\(\) => void pickAttachments\(\)\}/);
  // The typed-mention autocomplete is untouched.
  assert.ok(chat.includes("showSkillMenu"), "@ mentions still autocomplete");
});

test("the box is one hairline-bordered rounded surface, not a floating card", () => {
  const shell = lastRule(chatCss, ".composer-shell");
  assert.match(shell, /border:\s*1px solid var\(--line\)/);
  assert.match(shell, /border-radius:\s*12px/);
  assert.match(shell, /background:\s*var\(--surface\)/);
  assert.match(shell, /box-shadow:\s*none/);
  assert.match(lastRule(chatCss, ".composer-input"), /min-height:\s*46px/);
});

test("the strip is a slim plain row with nothing boxed left in it", () => {
  const bar = lastRule(chatCss, ".composer-bar");
  assert.match(bar, /min-height:\s*28px/);
  assert.match(bar, /border-top:\s*none/, "no boxes and no rules around the controls");
  assert.match(bar, /background:\s*transparent/);

  assert.match(lastRule(chatCss, ".composer-bar-right"), /margin-left:\s*auto/);
  // The narrow dock wraps the right group rather than pushing a control out of reach.
  assert.match(lastRule(chatCss, ".composer-bar-right"), /flex-wrap:\s*wrap/);

  // The tier segmented control was the only outlined thing in the row; it left with
  // the chips, and its rules must not linger to re-box a stray element.
  assert.ok(!chatCss.includes(".composer-bar .tier-chips"), "the tier rules left too");
});

test("the strip's colours are tokens, so it survives the light palette and the style modes", () => {
  const btn = lastRule(chatCss, ".composer-bar-btn");
  assert.match(btn, /color:\s*var\(--text-dim\)/);
  assert.doesNotMatch(btn, /#[0-9a-fA-F]{3,8}/, "a hard-coded slate is invisible on a light ground");
  const hover = lastRule(chatCss, ".composer-bar-btn:hover,\n.composer-bar-btn.active");
  assert.doesNotMatch(hover, /#[0-9a-fA-F]{3,8}/);
  assert.doesNotMatch(hover, /rgba\(/);
});

// ── Attachments ───────────────────────────────────────────────────────────────

/**
 * The owner's words were "add a + icon on the left so user can add images, or attach
 * anything, and it shows above like how in chatgpt/claude it shows". Before this, Attach
 * typed an `@` into the textarea: nothing was picked, nothing was previewed, and nothing
 * reached the model. These lock down the three halves of the real thing — the picker, the
 * chips above the input, and the paths travelling with the turn.
 */

test("the + menu's first row is Attach, and it opens the native file picker", () => {
  const listAt = popovers.indexOf('<div className="popover-item-list">');
  const attachAt = popovers.indexOf("Attach photos");
  const designAt = popovers.indexOf("Bhippi Design");
  assert.ok(attachAt > listAt, "the attach row is inside the first list");
  assert.ok(attachAt < designAt, "…and it is the first row in it");
  assert.ok(!popovers.includes(">Attach<"), "the bare old label is gone, not shadowed");

  // The picker itself is the plugin's, opened for many files with image filters first.
  assert.match(chat, /import \{ open \} from "@tauri-apps\/plugin-dialog";/);
  assert.match(chat, /await open\(\{ multiple: true, title: "Attach", filters: ATTACH_FILTERS \}\)/);
  const filters = chat.slice(chat.indexOf("const ATTACH_FILTERS"), chat.indexOf("conversationAttachments"));
  for (const extension of ["png", "jpg", "jpeg", "gif", "webp", "bmp"]) {
    assert.ok(filters.includes(`"${extension}"`), `${extension} should be pickable as an image`);
  }
  assert.ok(filters.includes('extensions: ["*"]'), "…and anything else can still be attached");
});

test("the previews sit above the textarea, inside the box", () => {
  const shellAt = chat.indexOf("composer-shell${streaming");
  const rowAt = chat.indexOf('<div className="composer-attachments"');
  const textareaAt = chat.indexOf("<textarea", shellAt);
  const barAt = chat.indexOf('<div className="composer-bar">');
  assert.ok(rowAt > shellAt, "the row is inside the box");
  assert.ok(rowAt < textareaAt, "…above the input");
  assert.ok(rowAt < barAt, "…and nowhere near the strip below it");

  // An image is a thumbnail; anything else is a card with a name and a size. Both are
  // removable, and the row wraps rather than pushing a chip out of reach.
  assert.match(chat, /<img className="composer-attachment-thumb" src=\{item\.data_url\}/);
  assert.ok(chat.includes("composer-attachment-name"));
  assert.ok(chat.includes("{item.size_label}"), "Rust renders the size, the page prints it");
  assert.ok(chat.includes("composer-attachment-remove"));
  assert.match(chat, /onClick=\{\(\) => removeAttachment\(item\.path\)\}/);

  const row = lastRule(chatCss, ".composer-attachments");
  assert.match(row, /flex-wrap:\s*wrap/);
  const thumb = lastRule(chatCss, ".composer-attachment-thumb");
  assert.match(thumb, /width:\s*56px/);
  assert.match(thumb, /object-fit:\s*cover/);
});

test("the attachment chips are painted in tokens, so both palettes work", () => {
  for (const selector of [
    ".composer-attachment",
    ".composer-attachment-glyph",
    ".composer-attachment-name",
    ".composer-attachment-size",
    ".composer-attachment-remove",
  ]) {
    const rule = lastRule(chatCss, selector);
    assert.doesNotMatch(rule, /#[0-9a-fA-F]{3,8}/, `${selector} hard-codes a colour`);
    assert.doesNotMatch(rule, /rgba?\(/, `${selector} hard-codes a colour`);
  }
});

test("a preview is described by Rust, and the paths travel with the turn", () => {
  // The asset protocol is off, so a thumbnail can only arrive as a data URL from a
  // command — and the page must not be the thing deciding what an image is.
  assert.match(apiTs, /attachmentPreview: \(path: string\) => ok\(commands\.attachmentPreview\(path\)\)/);
  assert.ok(ipcTs.includes('__TAURI_INVOKE("attachment_preview"'), "the binding is generated");
  assert.ok(ipcTs.includes('export type AttachmentKind = "image" | "file";'));
  assert.match(chat, /await api\.attachmentPreview\(path\)/);

  // send_chat_message carries the absolute paths; nothing is base64'd into the message.
  assert.ok(
    ipcTs.includes("caveman, attachments") && ipcTs.includes("attachments: string[] | null"),
    "the generated send binding takes the attachments",
  );
  assert.match(apiTs, /attachments\?: string\[\] \| null,/);
  assert.match(chat, /const sent = attachments\.map\(\(one\) => one\.path\);/);
  assert.match(chat, /sent\.length > 0 \? sent : null,/);
  assert.ok(!chat.includes("data_url,"), "a data URL is a preview, never something sent");
});

test("a sent draft loses its attachments; a queued one keeps them", () => {
  const send = chat.slice(chat.indexOf("const sendText = async"), chat.indexOf("const send = () => sendText()"));
  // The queue branch returns before the send, and must not clear the chips: the queued
  // message is dispatched from this same composer when the running turn finishes.
  const queueBranch = send.slice(send.indexOf("setQueuedMessages((prev) => [...prev, newQueued])"), send.indexOf("setSending(true)"));
  assert.ok(!queueBranch.includes("rememberAttachments"), "queuing must not drop the files");
  // The real send hands the paths over and then empties the draft.
  const afterSend = send.slice(send.indexOf("const pair = await api.sendMessage"));
  assert.match(afterSend, /rememberAttachments\(\[\]\);/);
});

// ── The composer drop-ups ─────────────────────────────────────────────────────

/**
 * The studio chat is a ~380–460px column beside the embedded Godot editor, which
 * is a NATIVE child window (ADR-0045). A popover that reaches past the column is
 * not merely ugly: the OS paints the game window over it and the panel reads as
 * cut in half. The owner hit this with the Effort panel, the `+` menu (spilling
 * LEFT into the sidebar) and the model list (spilling RIGHT into the viewport),
 * on both Claude and OpenCode — so containment has to live on the shared shell.
 */

/** Every top-level declaration block in the sheet, as `{ selector, body }`. */
function declarationBlocks(css) {
  const out = [];
  const re = /(?:^|\n)([^{}@\n][^{}]*)\{([^{}]*)\}/g;
  let match = re.exec(css);
  while (match) {
    // A selector is often preceded by its comment on the same capture; the comment
    // is not part of what the rule targets.
    out.push({ selector: match[1].replace(/\/\*[\s\S]*?\*\//g, "").trim(), body: match[2] });
    match = re.exec(css);
  }
  return out;
}

/** The composer popover family: the shared shell and the six panels on it. */
const isComposerPopover = (selector) =>
  /\.bhippi-popover|\.popover-|\.(provider|model|thinking|permission|options|ledger)-popover/.test(
    selector,
  );

test("the popover recipe is declared once, on the shared shell", () => {
  const frames = rulesFor(chatCss, ".bhippi-popover");
  assert.equal(frames.length, 1, "one recipe, not one per panel");
  const frame = frames[0];

  assert.match(frame, /border-radius:\s*10px/);
  assert.match(frame, /border:\s*1px solid var\(--line\)/);
  assert.match(frame, /background:\s*var\(--surface\)/);
  assert.match(frame, /var\(--lift-2\)/, "the same lift the rest of the floating chrome uses");
  // The inner top-edge highlight is what makes the edge read as glass.
  assert.match(
    frame,
    /inset 0 1px 0 var\(--canvas-sheen,\s*color-mix\(in srgb, var\(--text\) 12%, transparent\)\)/,
    "the sheen the lead is adding on .shell, with a working fallback until it lands",
  );

  // Nothing may re-paint the frame per panel — that is what made the Effort card
  // look like it came from a different app, and what the glass mode has to fight.
  for (const { selector, body } of declarationBlocks(chatCss)) {
    if (!isComposerPopover(selector) || selector.includes(".bhippi-popover")) continue;
    if (/^\.(popover-row|popover-head|popover-more|popover-search|popover-switch|popover-muted)/.test(selector)) {
      continue;
    }
    assert.doesNotMatch(body, /^\s*box-shadow:/m, `${selector} re-declares the frame's shadow`);
    assert.doesNotMatch(body, /^\s*border:\s*1px/m, `${selector} re-declares the frame's border`);
    assert.doesNotMatch(body, /^\s*background:\s*#/m, `${selector} paints its own ground`);
  }
});

test("the glass mode's own popover recipe is extended, not fought", () => {
  // tokens.css owns the glass look for everything that floats. If chat.css started
  // forcing a background with !important, glass would silently stop applying.
  const tokens = read("../src/styles/tokens.css");
  assert.ok(tokens.includes(':root[data-style-mode="glass"] .bhippi-popover'));
  assert.doesNotMatch(rulesFor(chatCss, ".bhippi-popover")[0], /!important/);
});

test("every composer drop-up opens upward, and cannot leave the column", () => {
  // The trigger is not the containing block: a 24px button sitting anywhere along
  // the strip would put `left: 0` wherever that button happens to be. The strip is,
  // and the strip is exactly as wide as the column.
  assert.match(lastRule(chatCss, ".composer-bar"), /position:\s*relative/);
  assert.match(lastRule(chatCss, ".composer-popover-anchor"), /position:\s*static/);

  const frame = rulesFor(chatCss, ".bhippi-popover")[0];
  assert.match(frame, /bottom:\s*calc\(100% \+ 8px\)/, "upward, above the strip");
  assert.match(frame, /max-width:\s*100%/, "never wider than the column");
  assert.doesNotMatch(frame, /100vw/, "the viewport is not the column — the Godot window owns it");
  assert.match(frame, /max-height:\s*min\(/, "and never taller than the dock");

  // Left-group triggers hang from the left edge, right-group ones from the right.
  const left = lastRule(chatCss, ".composer-bar .composer-bar-left .bhippi-popover");
  assert.match(left, /left:\s*0/);
  assert.match(left, /right:\s*auto/);
  const right = lastRule(chatCss, ".composer-bar .composer-bar-right .bhippi-popover");
  assert.match(right, /right:\s*0/);
  assert.match(right, /left:\s*auto/);
  // Three classes each, because multi-workspace.css loads later and caps the shell
  // at a flat 320px — wider than a narrow session panel.
  for (const rule of [left, right]) assert.match(rule, /max-width:\s*100%/);
});

test("every panel opens upward — no panel opts out of the drop-up", () => {
  for (const { selector, body } of declarationBlocks(chatCss)) {
    if (!/-popover\b/.test(selector) || !isComposerPopover(selector)) continue;
    assert.doesNotMatch(body, /^\s*top:/m, `${selector} would open downward`);
    if (/^\s*bottom:/m.test(body)) {
      assert.match(body, /bottom:\s*calc\(100%/, `${selector} must hang off the strip`);
    }
  }
});

test("no popover is wider than the narrow column can carry", () => {
  for (const { selector, body } of declarationBlocks(chatCss)) {
    if (!isComposerPopover(selector)) continue;
    // Any px in a width or min-width, `min(216px, 100%)` included.
    for (const [, px] of body.matchAll(/(?:^|[\s;])(?:min-)?width:[^;]*?([\d.]+)px/g)) {
      assert.ok(
        Number(px) <= 320,
        `${selector} pins ${px}px — a 380px dock cannot hold that with its gutters`,
      );
    }
  }
});

test("the Effort panel is a small card, not the tall hand-painted one", () => {
  const thinking = lastRule(chatCss, ".thinking-popover");
  assert.match(thinking, /min-width:\s*min\(216px, 100%\)/, "~200–240px, not 268px, and clamped");
  assert.doesNotMatch(thinking, /^\s*height:/m, "no fixed height");
  assert.doesNotMatch(thinking, /border-radius:/, "the radius comes from the shared recipe");
  assert.doesNotMatch(thinking, /background:/, "…and so does the ground");

  // Title row names the level; the scale under the rail is three words.
  assert.match(popovers, /<span className="thinking-label">Effort<\/span>/);
  assert.match(popovers, /<strong className="thinking-val">\{currentStep\.name\}<\/strong>/);
  const scaleAt = popovers.indexOf('className="thinking-scale-row"');
  assert.ok(scaleAt > 0, "the scale row should exist");
  const scale = popovers.slice(scaleAt, scaleAt + 260);
  for (const label of ["Faster", "Balanced", "Smarter"]) {
    assert.ok(scale.includes(`<span>${label}</span>`), `the scale is missing ${label}`);
  }
  // The old two-ended legend and the `?` bubble left with the tall padding.
  assert.ok(!popovers.includes("thinking-ends-row"));
  assert.ok(!popovers.includes("thinking-help"));

  // Escape and click-outside still close it — that is the whole popover contract.
  assert.match(popovers, /if \(event\.key === "Escape"\)/);
  assert.match(popovers, /window\.addEventListener\("pointerdown", onPointerDown, true\)/);
});

test("a model row is a name and one muted word, and the list scrolls inside the card", () => {
  // The blue capability dot meter per row was the noise the owner named.
  for (const gone of [
    "model-dot-meter",
    "meter-dot",
    "model-row-prefix",
    "model-badge-paid",
    "model-source-text",
  ]) {
    assert.ok(!popovers.includes(gone), `${gone} should be gone from the markup`);
    assert.ok(!chatCss.includes(`.${gone}`), `${gone} should be gone from the sheet`);
  }
  assert.ok(!popovers.includes("dots:"), "the dot data left with the meter");

  // SPA-406: the row prints the short name; the backend prefix is the group head above it.
  assert.match(
    popovers,
    /<span className="popover-row-name model-id-text">\{shortModelName\(item\.id\)\}<\/span>/,
  );
  assert.match(popovers, /export function shortModelName/);
  assert.match(popovers, /export function groupModels/);
  assert.match(popovers, /rowMeta \? <span className="model-meta-text">\{rowMeta\}<\/span> : null/);
  assert.match(popovers, /\{isSelected \? <IconCheck size=\{14\} \/> : null\}/);
  // `(1M)` is the context window, so it reads as meta — but the full id is still
  // what gets selected and compared.
  assert.match(popovers, /export function splitModelMeta/);
  assert.match(popovers, /onSelect\(item\.id\);/);

  assert.match(lastRule(chatCss, ".model-popover .popover-item-list"), /max-height:\s*min\(50vh/);
  // One shell for every catalogue: nothing about containment may be per provider.
  assert.ok(!chatCss.includes(".model-popover.opencode"));
});

test("the + menu hangs off the column's left edge and its toggles are switches", () => {
  const options = lastRule(chatCss, ".options-popover");
  assert.doesNotMatch(options, /right:\s*0/, "pinning it right pushed its left half under the sidebar");
  assert.match(options, /max-width:\s*min\(268px, 100%\)/);
  assert.match(lastRule(chatCss, ".options-popover .popover-item-list"), /max-height:\s*min\(/);

  // Six stacked "On/Off" words were most of the menu's height.
  assert.ok(!popovers.includes("toggle-text"), "the word pair is gone, not shadowed");
  assert.match(popovers, /className=\{`popover-switch\$\{designOn \? " on" : ""\}`\}/);
  assert.match(popovers, /aria-pressed=\{Boolean\(designOn\)\}/, "a switch still says its state");
  assert.match(lastRule(chatCss, ".popover-switch"), /width:\s*24px/);

  // Attach is still the first row, and the stepper is still inline.
  assert.ok(popovers.indexOf("Attach photos") < popovers.indexOf("Bhippi Design"));
  assert.ok(popovers.includes("text-size-stepper"));
});

test("a long model id cannot wrap the strip onto a second line", () => {
  const label = lastRule(chatCss, ".composer-bar-btn .model-trigger-text");
  assert.match(label, /max-width:\s*104px/);
  assert.match(label, /text-overflow:\s*ellipsis/);
  assert.match(label, /white-space:\s*nowrap/);
  // …and the whole name is one hover away, on both pickers.
  assert.match(popovers, /aria-label=\{`Model: \$\{activeLabel\}`\}[\s\S]{0,400}title=\{activeLabel\}/);
  assert.match(popovers, /title=\{active\?\.label \?\? "Select provider"\}/);
});

test("the popovers are painted in tokens, so both palettes and every style mode work", () => {
  for (const selector of [
    ".bhippi-popover",
    ".composer-bar .composer-bar-left .bhippi-popover",
    ".composer-bar .composer-bar-right .bhippi-popover",
    ".thinking-popover",
    ".model-popover",
    ".options-popover",
    ".permission-popover",
    ".ledger-popover",
    ".popover-switch",
    ".popover-head-simple",
    ".popover-item-list",
    ".popover-row-btn",
    ".model-meta-text",
    ".popover-search-box",
    ".popover-more-btn",
    ".thinking-scale-row",
    ".thinking-rail-bg",
    ".thinking-pill-knob",
  ]) {
    const rule = lastRule(chatCss, selector);
    assert.doesNotMatch(rule, /#[0-9a-fA-F]{3,8}\b/, `${selector} hard-codes a colour`);
    assert.doesNotMatch(rule, /rgba?\(/, `${selector} hard-codes a colour`);
  }
});
