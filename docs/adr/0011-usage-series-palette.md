# ADR-0011 — A categorical series palette for the usage chart

**Status:** accepted · **Date:** 2026-08-26
**Amends:** `04-PAGES.md` §Design contract, ADR-0009 (“one accent, amber → orange”)

## Context

Settings › Usage answers one question — *where did the tokens and the money go* — and
the answer is per provider. Until now the panel drew a single amber column chart of the
daily total, so the one thing the screen exists to show had to be read off a table
underneath it.

The design contract says Bhippi has **one** accent. That rule is right for chrome: it
keeps buttons, focus rings, and gauges from competing. It cannot survive contact with a
chart that carries five or six series at once, because in a chart colour is not
decoration — it is the identity of the series, and identity cannot be encoded in one hue.

Two other in-repo scales already exist alongside the accent for exactly this reason: the
budget gauge ramp (`--gauge-0..3`) and the status colours. The precedent is that a scale
which *means* something gets its own values.

## Decision

Add a **categorical series palette** used only by data marks in Settings › Usage. It is
not a brand palette: it never appears in chrome, buttons, links, or gauges.

The eight steps and their order are taken unchanged from the validated dark-mode
reference palette:

| Slot | Hue | Value |
|------|-----|-------|
| 1 | blue | `#3987e5` |
| 2 | orange | `#d95926` |
| 3 | aqua | `#199e70` |
| 4 | yellow | `#c98500` |
| 5 | magenta | `#d55181` |
| 6 | green | `#008300` |
| 7 | violet | `#9085e9` |
| 8 | red | `#e66767` |

Validated as a set against Bhippi's chart surface (`--surface`, `#171614`):

```
Lightness band      PASS  all 8 inside L 0.48–0.67
Chroma floor        PASS  all 8 >= 0.1
CVD separation      PASS  worst adjacent #c98500↔#199e70 ΔE 8.4 (protan)
Normal-vision floor PASS  worst adjacent #d55181↔#c98500 ΔE 19.3
Contrast vs surface PASS  all 8 >= 3:1
```

**The order is load-bearing.** Re-ordering these same eight hues fails CVD separation —
a warm-led five-slot variant was measured at ΔE 1.6 (deutan) and rejected. Anyone
changing this table re-runs the validator before, not after.

Three rules bind the palette:

1. **A slot follows the provider, never its rank.** `ProviderUsage.color_slot` is the
   provider's index in `bhippi-providers::CATALOG`, computed in Rust. A backend that
   goes quiet must not hand its colour to whoever replaced it at the top of the table.
2. **Colour is never the only signal.** Every series is direct-labelled in the legend
   rows with its provider logo and name, and every figure on the panel is also present
   as text in the breakdown table.
3. **Chrome keeps the single accent.** The amber accent still owns selection, focus, and
   the budget ring. The chart does not use `--accent` for a series, so “the amber one”
   never ambiguously means both “selected” and “Claude Code”.

## Consequences

- `tokens.css` gains `--series-1..8`, documented as chart-only.
- The usage panel can show composition over time, which is what the screen is for.
- Nine or more spending providers fold into a single “Other” series rather than
  generating a ninth hue.
- Light mode, when it lands, needs the light column of the same reference table plus a
  re-run of the validator against the light surface. The dark steps are not an automatic
  flip.
