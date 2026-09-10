version: 1
domain: web
title: Dynamic pages
when: interactive pages: state, loading, forms, tables, libraries
tags: dynamic, interactive, state, javascript, react, library, cdn, form, table, list, loading, skeleton, optimistic, live-region, keyboard, app, widget

# Dynamic pages

<!-- section: state -->
## 1. State in one place

One source of truth per page (a store, a reducer, a single object); the DOM renders from it.
Derived values are computed, never stored twice. Every state change that the user watches
produces a visible change within `--t-quick`. A page opens in a realistic working state —
real data where it exists, plainly marked example rows otherwise — never an empty shell.

<!-- section: loading -->
## 2. Loading and optimism

A skeleton the shape of the content, not a spinner in a void; a loading button keeps its
width and its label. Optimistic updates for actions that almost always succeed (a toggle, a
rename), with a visible rollback and message when they do not. Partial results render as
they arrive; the page says what is still coming. Never block the whole page for one region.

<!-- section: forms -->
## 3. Forms

Label above, hint below, error replacing the hint with an icon and a message; never a
placeholder as the label. Validate on blur, re-validate on input once an error exists.
Submit with Enter; the primary button is the last thing in the tab order of the form. Keep
what the user typed on error. Long forms are sections with headings, not steps, unless a step
genuinely gates the next.

<!-- section: tables -->
## 4. Tables and lists

Header 11 px, dim, sticky; rows 32 px in a tool, 44 px on touch; numbers right-aligned and
tabular; zebra striping only past about fifteen rows. Sort and filter from the header, with
the active sort shown by a glyph and a label. A list past a few hundred rows is virtualised.
Selection is a checkbox column, never a row colour alone. Empty, loading and error states
inside the table's own box.

<!-- section: keyboard -->
## 5. Keyboard model

Tab moves between controls, arrows move within a control (a list, a menu, a tab strip),
Escape closes the nearest layer, Enter activates. Focus is trapped in a modal and returns to
its trigger on close. Live regions announce results counts and errors.

<!-- section: libraries -->
## 6. When a library earns its place

Most pages need none. A library is loaded when it carries real weight — a chart engine, a
syntax highlighter, React for a genuinely stateful app — from an allowed CDN, pinned to an
exact version, as a UMD build defining a global, placed **before** the inline script that
uses it. The library's stylesheet is inlined (external stylesheets are usually blocked). The
page's own CSS, JS, images and data ship with the page: assets as data URIs, fonts from
Google Fonts. Never paste a library's source; never hand-write a stand-in for one.

<!-- section: storage -->
## 7. Storage and persistence

Browser storage is for per-viewer conveniences — a remembered tab, a draft — wrapped in
try/catch and never relied on. Shared or durable state goes through the app's real store.
Nothing sensitive in a URL.

<!-- section: sandbox -->
## 8. Sandbox realities

A page may run where downloads, external fetches and cross-origin frames are blocked. Offer a
file through a real save path, not a plain link; fetch only what the host allows; render
correctly with every external call failed.
