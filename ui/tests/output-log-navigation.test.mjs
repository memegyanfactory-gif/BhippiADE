import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

test("typed console rows and model query share the same Rust filter", () => {
  const telemetry = fs.readFileSync(
    new URL("../../crates/bhippi-app/src/engine/telemetry.rs", import.meta.url),
    "utf8",
  );
  assert.match(telemetry, /engine_console_rows[\s\S]*?filtered_rows/);
  assert.match(telemetry, /console_answer[\s\S]*?filtered_rows/);
  assert.match(telemetry, /file: Option<String>/);
  assert.match(telemetry, /line: Option<u32>/);
});

test("a source row opens the exact project file and line, with a missing-file error", () => {
  const log = fs.readFileSync(new URL("../src/engine/EngineOutputLog.tsx", import.meta.url), "utf8");
  const workbench = fs.readFileSync(new URL("../src/workbench/Workbench.tsx", import.meta.url), "utf8");
  const editor = fs.readFileSync(new URL("../src/workbench/CodeView.tsx", import.meta.url), "utf8");
  assert.match(log, /requestOpenWorkspaceFile\(line\.source!\.path, line\.source!\.line\)/);
  assert.match(workbench, /api\.readFile\(path\)/);
  assert.match(workbench, /Could not open \$\{path\}:\$\{line\}/);
  assert.match(editor, /setSelectionRange\(offset/);
  assert.match(editor, /scrollTop/);
});
