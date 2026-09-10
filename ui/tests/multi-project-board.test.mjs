import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  cleanProjectPath,
  flattenBoardSessions,
  groupSessionsByProject,
  projectForSession,
  projectHue,
} from "../src/workspace/multiProjectBoard.ts";
import { readWorkspaceMode } from "../src/workspace/workspaceMode.ts";

function project(name, path, extra = {}) {
  return {
    name,
    path,
    is_git_repository: false,
    branch: null,
    active: false,
    last_opened_at: 0,
    ...extra,
  };
}

function session(id, projectPath, updatedAt, extra = {}) {
  return {
    id,
    project_path: projectPath,
    kind: "chat",
    title: id,
    provider: null,
    provider_label: null,
    status: "idle",
    created_at: updatedAt,
    updated_at: updatedAt,
    turn_count: 0,
    ...extra,
  };
}

test("every project gets a column, newest chat first", () => {
  const projects = [project("Alpha", "C:/games/alpha"), project("Beta", "C:/games/beta")];
  const columns = groupSessionsByProject(projects, [
    session("a-old", "C:/games/alpha", "2026-09-01T10:00:00Z"),
    session("b-one", "C:/games/beta", "2026-09-02T10:00:00Z"),
    session("a-new", "C:/games/alpha", "2026-09-03T10:00:00Z"),
  ]);

  assert.deepEqual(
    columns.map((column) => column.project.name),
    ["Alpha", "Beta"],
  );
  assert.deepEqual(
    columns[0].sessions.map((one) => one.id),
    ["a-new", "a-old"],
  );
  assert.deepEqual(
    columns[1].sessions.map((one) => one.id),
    ["b-one"],
  );
});

test("a project with no chats still gets its column", () => {
  const columns = groupSessionsByProject([project("Empty", "C:/games/empty")], []);
  assert.equal(columns.length, 1);
  assert.deepEqual(columns[0].sessions, []);
});

test("a session whose project has been forgotten lands on no column", () => {
  const columns = groupSessionsByProject(
    [project("Alpha", "C:/games/alpha")],
    [session("orphan", "C:/games/gone", "2026-09-01T10:00:00Z")],
  );
  assert.deepEqual(columns[0].sessions, []);
});

test("path spelling never splits one project into two columns", () => {
  const columns = groupSessionsByProject(
    [project("Alpha", "C:\\games\\alpha")],
    [session("a", "//?/C:/Games/Alpha/", "2026-09-01T10:00:00Z")],
  );
  assert.equal(columns.length, 1);
  assert.deepEqual(
    columns[0].sessions.map((one) => one.id),
    ["a"],
  );
  assert.equal(cleanProjectPath("C:\\Games\\Alpha\\"), "c:/games/alpha");
});

test("the board is one flat, newest-first set of windows across every project", () => {
  const columns = groupSessionsByProject(
    [project("Alpha", "C:/games/alpha"), project("Beta", "C:/games/beta")],
    [
      session("a-new", "C:/games/alpha", "2026-09-03T10:00:00Z"),
      session("a-old", "C:/games/alpha", "2026-09-01T10:00:00Z"),
      session("b-new", "C:/games/beta", "2026-09-04T10:00:00Z"),
      session("b-old", "C:/games/beta", "2026-09-02T10:00:00Z"),
    ],
  );

  assert.deepEqual(
    flattenBoardSessions(columns).map((one) => one.id),
    ["b-new", "a-new", "b-old", "a-old"],
  );
});

test("the board hides nothing: every project on the list contributes its windows", () => {
  const columns = groupSessionsByProject(
    [project("Alpha", "C:/games/alpha"), project("Beta", "C:/games/beta")],
    [
      session("a-one", "C:/games/alpha", "2026-09-03T10:00:00Z"),
      session("b-one", "C:/games/beta", "2026-09-04T10:00:00Z"),
    ],
  );

  assert.deepEqual(
    flattenBoardSessions(columns).map((one) => one.id),
    ["b-one", "a-one"],
  );
});

test("flattening a board with no projects on it yields no windows", () => {
  assert.deepEqual(flattenBoardSessions([]), []);
});

test("a project's colour is stable across path spelling and never depends on its neighbours", () => {
  assert.equal(projectHue("C:\\games\\alpha"), projectHue("//?/C:/Games/Alpha/"));
  assert.notEqual(projectHue("C:/games/alpha"), projectHue("C:/games/beta"));
  const hue = projectHue("C:/games/alpha");
  assert.ok(Number.isInteger(hue) && hue >= 0 && hue < 360, `hue out of range: ${hue}`);
});

test("a window finds the project it belongs to, whatever the path spelling", () => {
  const projects = [project("Alpha", "C:\\games\\alpha"), project("Beta", "C:/games/beta")];
  assert.equal(
    projectForSession(projects, session("a", "//?/C:/Games/Alpha/", "2026-09-01T10:00:00Z"))?.name,
    "Alpha",
  );
  assert.equal(
    projectForSession(projects, session("gone", "C:/games/vanished", "2026-09-01T10:00:00Z")),
    null,
  );
});

test("the saved layout mode survives, and an unknown one falls back to single", () => {
  assert.equal(readWorkspaceMode("single"), "single");
  assert.equal(readWorkspaceMode("multi"), "multi");
  assert.equal(readWorkspaceMode("multiproject"), "multiproject");
  assert.equal(readWorkspaceMode(null), "single");
  assert.equal(readWorkspaceMode("engine"), "single");
});

test("the board's windows hang from a real height chain, exactly like Multi mode's", () => {
  const workspace = readFileSync(
    new URL("../src/styles/multi-workspace.css", import.meta.url),
    "utf8",
  );
  const screen = readFileSync(new URL("../src/screens/ProjectsScreen.tsx", import.meta.url), "utf8");

  // A pane body is `flex: 1; min-height: 0`, so one content-sized ancestor collapses every
  // window on the canvas to its title bar. The wrapper the board renders into shipped with
  // no rule at all once and did exactly that.
  const wrapper = /\.projects-multi-container,\s+\.projects-multiproject-container\s*\{([\s\S]*?)\}/.exec(
    workspace,
  );
  assert.ok(wrapper, "both Multi containers must be declared together");
  assert.match(wrapper[1], /height: 100%/);
  assert.match(wrapper[1], /min-height: 0/);
  assert.match(wrapper[1], /display: flex/);
  assert.match(screen, /className="projects-multiproject-container"/);

  // And the pane itself must be the shared one, so the chat inside a board window is the
  // same chat a Multi-mode window draws — the tile rules all key off this class.
  assert.match(workspace, /\.multi-session-workspace \.chat\s*\{[\s\S]*?height: 100%/);
});

test("the board draws no chrome of its own, so it looks exactly like Multi mode", () => {
  const board = readFileSync(
    new URL("../src/workspace/MultiProjectWorkspace.tsx", import.meta.url),
    "utf8",
  );
  const css = readFileSync(new URL("../src/styles/multi-project.css", import.meta.url), "utf8");

  // The board once carried a bar of its own — a title, a window count, a chip per project
  // and its own buttons. Multi mode has no such strip, so neither may this.
  for (const gone of [
    "multi-project-bar",
    "multi-project-chip",
    "multi-project-title",
    "multi-project-count",
    "multi-project-empty",
    "multi-project-board",
    "multi-project-canvas",
  ]) {
    assert.ok(!board.includes(gone), `${gone} must be gone from the board`);
    assert.ok(!css.includes(gone), `${gone} must be gone from the stylesheet`);
  }

  // What is left is one element: the shared window manager, exactly as Multi mode renders
  // it — plus the per-window project badge, which lives on the window frame, not on a bar.
  assert.match(board, /return \(\s*<MultiSessionWorkspace/);
  assert.match(css, /\.session-panel\.has-project::after/);
});
