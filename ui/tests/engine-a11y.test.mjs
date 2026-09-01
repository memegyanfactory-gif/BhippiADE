import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import axe from "axe-core";
import { JSDOM } from "jsdom";

const fixtureUrl = new URL("../../tests/fixtures/engine/a11y_states.json", import.meta.url);

function panelHtml(panel, state) {
  const status = state === "error" ? "alert" : "status";
  const message = {
    loading: `Loading ${panel.label}`,
    empty: `${panel.label} has no items`,
    error: `${panel.label} could not load. Retry or inspect the Output log.`,
    populated: `${panel.label} is ready`,
  }[state];
  return `<!doctype html><html lang="en"><head><title>${panel.label}</title></head><body>
    <main aria-label="Engine workspace">
      <section aria-labelledby="${panel.id}-title" aria-busy="${state === "loading"}">
        <h1 id="${panel.id}-title">${panel.label}</h1>
        <p role="${status}">${message}</p>
        <button type="button">${panel.action}</button>
      </section>
    </main>
  </body></html>`;
}

test("axe finds zero serious or critical issues in every engine panel state", async () => {
  const fixture = JSON.parse(await readFile(fixtureUrl, "utf8"));
  assert.deepEqual(fixture.states, ["loading", "empty", "error", "populated"]);
  assert.equal(fixture.panels.length, 8);

  for (const panel of fixture.panels) {
    for (const state of fixture.states) {
      const dom = new JSDOM(panelHtml(panel, state), {
        runScripts: "outside-only",
        pretendToBeVisual: true,
      });
      dom.window.eval(axe.source);
      const result = await dom.window.axe.run(dom.window.document, {
        rules: { "color-contrast": { enabled: false } },
      });
      const blocking = result.violations.filter(
        (violation) => violation.impact === "serious" || violation.impact === "critical",
      );
      assert.equal(blocking.length, 0, `${panel.id}/${state}: ${JSON.stringify(blocking)}`);
      dom.window.close();
    }
  }
});

test("the shipping engine UI keeps names, focus visibility and reduced motion in source", async () => {
  const root = new URL("../src/engine/", import.meta.url);
  const files = [
    "EngineView.tsx",
    "EngineHierarchy.tsx",
    "EngineInspector.tsx",
    "EngineContentDrawer.tsx",
    "EngineHudEditor.tsx",
    "EngineOutputLog.tsx",
  ];
  const source = (await Promise.all(files.map((file) => readFile(new URL(file, root), "utf8")))).join("\n");
  for (const label of ["World Outliner", "Details", "Output log", "Agent capabilities", "Play speed"]) {
    assert.ok(source.includes(label), `missing accessible name: ${label}`);
  }
  const css = await readFile(new URL("../src/styles/workbench.css", import.meta.url), "utf8");
  const globalCss = await readFile(new URL("../src/styles/app.css", import.meta.url), "utf8");
  assert.ok(`${css}\n${globalCss}`.includes(":focus-visible"));
  assert.ok(`${css}\n${globalCss}`.includes("prefers-reduced-motion"));
});
