import assert from "node:assert/strict";
import test from "node:test";
import {
  antigravityDisplayName,
  antigravityFamilyId,
  antigravitySpeedForEffort,
  collapseAntigravityModels,
  resolveAntigravitySlug,
} from "../src/lib/antigravityModels.ts";

const CATALOG = [
  "gemini-3.8-flash-high",
  "gemini-3.8-flash-medium",
  "gemini-3.8-flash-low",
  "gemini-3.1-pro-high",
  "gemini-3.1-pro-low",
  "claude-sonnet-4-6",
  "claude-opus-4-6-thinking",
  "gpt-oss-120b-medium",
];

test("speed variants collapse to one family name", () => {
  const rows = collapseAntigravityModels(CATALOG);
  assert.deepEqual(
    rows.map((row) => row.label),
    [
      "Gemini 3.8 Flash",
      "Gemini 3.1 Pro",
      "Claude Sonnet 4.6",
      "Claude Opus 4.6",
      "GPT-OSS 120B",
    ],
  );
  assert.ok(!rows.some((row) => /high|medium|low/i.test(row.label)));
});

test("display names are the model and version, not the slug", () => {
  assert.equal(antigravityDisplayName("gemini-3.8-flash-high"), "Gemini 3.8 Flash");
  assert.equal(antigravityDisplayName("claude-sonnet-4-6"), "Claude Sonnet 4.6");
  assert.equal(antigravityFamilyId("gemini-3.8-flash-medium"), "gemini-3.8-flash");
});

test("composer speed picks the matching Antigravity slug", () => {
  assert.equal(antigravitySpeedForEffort("fast"), "low");
  assert.equal(antigravitySpeedForEffort("medium"), "medium");
  assert.equal(antigravitySpeedForEffort("balanced"), "high");
  assert.equal(antigravitySpeedForEffort("ultra"), "high");

  assert.equal(
    resolveAntigravitySlug("gemini-3.8-flash", "fast", CATALOG),
    "gemini-3.8-flash-low",
  );
  assert.equal(
    resolveAntigravitySlug("gemini-3.8-flash-high", "medium", CATALOG),
    "gemini-3.8-flash-medium",
  );
  assert.equal(
    resolveAntigravitySlug("gemini-3.8-flash", "ultra", CATALOG),
    "gemini-3.8-flash-high",
  );
  assert.equal(
    resolveAntigravitySlug("gemini-3.1-pro", "medium", CATALOG),
    "gemini-3.1-pro-high",
  );
  assert.equal(
    resolveAntigravitySlug("claude-sonnet-4-6", "fast", CATALOG),
    "claude-sonnet-4-6",
  );
  assert.equal(
    resolveAntigravitySlug("gpt-oss-120b", "ultra", CATALOG),
    "gpt-oss-120b-medium",
  );

  // Friendly display names from picker / favorites must resolve to real slugs
  assert.equal(
    resolveAntigravitySlug("Gemini 3.8 Flash", "balanced", CATALOG),
    "gemini-3.8-flash-high",
  );
  assert.equal(
    resolveAntigravitySlug("Gemini 3.8 Flash", "fast", CATALOG),
    "gemini-3.8-flash-low",
  );
  assert.equal(
    resolveAntigravitySlug("Gemini 3.8 Flash", "medium", CATALOG),
    "gemini-3.8-flash-medium",
  );
  assert.equal(
    resolveAntigravitySlug("Claude Sonnet 4.6", "balanced", CATALOG),
    "claude-sonnet-4-6",
  );
  assert.equal(
    resolveAntigravitySlug("GPT-OSS 120B", "balanced", CATALOG),
    "gpt-oss-120b-medium",
  );
});

test("supported speeds detection per model", async () => {
  const { getSupportedSpeedsForAntigravityModel } = await import("../src/lib/antigravityModels.ts");
  assert.deepEqual(
    getSupportedSpeedsForAntigravityModel("Gemini 3.8 Flash", CATALOG),
    ["low", "medium", "high"],
  );
  assert.deepEqual(
    getSupportedSpeedsForAntigravityModel("Gemini 3.1 Pro", CATALOG),
    ["low", "high"],
  );
  assert.deepEqual(
    getSupportedSpeedsForAntigravityModel("Claude Sonnet 4.6", CATALOG),
    [],
  );
});
