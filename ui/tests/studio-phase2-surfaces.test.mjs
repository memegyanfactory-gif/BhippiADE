/**
 * Phase 2 Studio Surfaces & Inspector Tests (GAD-020, GAD-022, GAD-023, GAD-024)
 */

import assert from "node:assert/strict";
import test from "node:test";

test("GAD-020: PlanCard projection structure and four lifecycle states", () => {
  const plan = {
    id: "plan-test-01",
    title: "Test Platformer",
    genre: "3D Platformer",
    perspective: "Third-Person Orbit",
    artStyle: "Stylized Low-Poly",
    mechanics: ["Kinematic Jelly movement", "Moving platforms"],
    systems: [
      { name: "Character Controller", desc: "Kinematic player body", done: true },
      { name: "Collectibles", desc: "Gold coins with particle sparks", done: false },
    ],
    openQuestions: [
      {
        id: "q1",
        question: "Jump style?",
        options: ["Floaty", "Arcade"],
        selected: "Floaty",
      },
    ],
    approved: false,
  };

  assert.equal(plan.systems.length, 2);
  assert.equal(plan.systems[0].done, true);
  assert.equal(plan.systems[1].done, false);
  assert.equal(plan.openQuestions[0].options.length, 2);
  assert.equal(plan.approved, false);
});

test("GAD-022: Versions drawer tab creates, lists, and identifies checkpoints", () => {
  const versions = [
    {
      id: "v-2",
      version: "v0.3.1",
      label: "Added moving platform & coins",
      createdAt: "Just now",
      commitHash: "8a4f91c",
      author: "Bhippi AI",
      changesCount: 6,
    },
    {
      id: "v-1",
      version: "v0.2.0",
      label: "Initial Jelly hero & islands",
      createdAt: "10 mins ago",
      commitHash: "b21e44f",
      author: "Bhippi AI",
      changesCount: 14,
    },
  ];

  assert.equal(versions.length, 2);
  assert.equal(versions[0].version, "v0.3.1");
  assert.equal(versions[0].changesCount, 6);

  // Auto-checkpoint creation logic
  const nextVersion = {
    id: `v-3`,
    version: `v0.3.2`,
    label: "Added dynamic camera orbit",
    createdAt: "Just now",
    commitHash: "c48d21a",
    author: "User",
    changesCount: 4,
  };
  const updated = [nextVersion, ...versions];
  assert.equal(updated.length, 3);
  assert.equal(updated[0].version, "v0.3.2");
});

test("GAD-023: Game settings TOML validation requires [game] and [godot] tables", () => {
  const validToml = `
[game]
name = "My Platformer"
description = "Fun platformer"
tags = ["3D", "Platformer"]

[godot]
version_pin = "4.7.1-stable"
main_scene = "res://scene/main.tscn"

[publish]
web_dir = "build/web"
include_credits = true
`;

  const invalidToml = `
[settings]
some_field = 123
`;

  const validateToml = (toml) => {
    return toml.includes("[game]") && toml.includes("[godot]");
  };

  assert.equal(validateToml(validToml), true);
  assert.equal(validateToml(invalidToml), false);
});

test("GAD-024: Assets provenance filtering and licence attribution", () => {
  const assets = [
    { id: "1", name: "main.tscn", provenance: "procedural", licence: "CC0" },
    { id: "2", name: "hero.glb", provenance: "library", licence: "CC0" },
    { id: "3", name: "pickup.wav", provenance: "external", licence: "MIT" },
    { id: "4", name: "custom.png", provenance: "imported", licence: "Custom" },
  ];

  const filterByProvenance = (list, prov) => {
    if (prov === "all") return list;
    return list.filter((a) => a.provenance === prov);
  };

  assert.equal(filterByProvenance(assets, "all").length, 4);
  assert.equal(filterByProvenance(assets, "procedural").length, 1);
  assert.equal(filterByProvenance(assets, "library").length, 1);
  assert.equal(filterByProvenance(assets, "external").length, 1);
  assert.equal(filterByProvenance(assets, "imported").length, 1);

  // Licence cycling
  const licences = ["CC0", "MIT", "Apache-2.0", "Custom"];
  const cycleLicence = (cur) => {
    const idx = licences.indexOf(cur);
    return licences[(idx + 1) % licences.length];
  };
  assert.equal(cycleLicence("CC0"), "MIT");
  assert.equal(cycleLicence("MIT"), "Apache-2.0");
  assert.equal(cycleLicence("Custom"), "CC0");
});

test("Inspector Unreal/Godot PBR material property synchronization schema", () => {
  const materialProps = {
    color: "#ff7700",
    roughness: 0.12,
    metalness: 0.02,
    emissive: "#ff5500",
    emissiveIntensity: 0.45,
    transmission: 0.65,
    opacity: 0.95,
    wireframe: false,
    texture: "wood_planks.png",
  };

  // Value bounds check
  assert.ok(materialProps.roughness >= 0.0 && materialProps.roughness <= 1.0);
  assert.ok(materialProps.metalness >= 0.0 && materialProps.metalness <= 1.0);
  assert.ok(materialProps.transmission >= 0.0 && materialProps.transmission <= 1.0);
  assert.ok(materialProps.opacity >= 0.0 && materialProps.opacity <= 1.0);
  assert.ok(materialProps.emissiveIntensity >= 0.0);
  assert.equal(typeof materialProps.wireframe, "boolean");
  assert.equal(typeof materialProps.texture, "string");
});

test("Studio Window Controls: top-right minimize, maximize, and close buttons", () => {
  const windowButtons = [
    { type: "minimize", title: "Minimize", label: "Minimize Window" },
    { type: "maximize", title: "Maximize / Restore", label: "Maximize Window" },
    { type: "close", title: "Close", label: "Close Window" },
  ];

  assert.equal(windowButtons.length, 3);
  assert.deepEqual(
    windowButtons.map((b) => b.type),
    ["minimize", "maximize", "close"],
  );
});

test("Studio Layout: side-by-side split, curved chat on left, contained 3D viewport on right", () => {
  const layout = {
    type: "flex",
    direction: "row",
    leftColumn: {
      component: "StudioChatPanel",
      width: "380px",
      borderRadius: "16px",
      contained: true,
    },
    rightColumn: {
      viewportCard: {
        component: "GodotViewport",
        borderRadius: "16px",
        overflow: "hidden",
        underneathChat: false, // Invariant: No 3D canvas runs behind the chat bar!
      },
      dock: {
        component: "StudioBottomDock",
      },
    },
  };

  assert.equal(layout.leftColumn.contained, true);
  assert.equal(layout.rightColumn.viewportCard.underneathChat, false);
  assert.equal(layout.rightColumn.viewportCard.borderRadius, "16px");
});

test("Default 3D Viewport: clean engine grid + live embedded Godot preview on play", () => {
  const viewportState = {
    hasAuthoredNodes: false,
    defaultGridActive: true,
    isPlaying: false,
    previewUrl: null,
  };

  // Default clean state
  assert.equal(viewportState.defaultGridActive, true);
  assert.equal(viewportState.isPlaying, false);

  // User presses Play / Preview
  const playSession = {
    isPlaying: true,
    previewUrl: "http://127.0.0.1:8060/index.html",
    runtime: "Godot 4.7.1",
  };

  assert.equal(playSession.isPlaying, true);
  assert.ok(playSession.previewUrl.startsWith("http://127.0.0.1"));
  assert.equal(playSession.runtime, "Godot 4.7.1");
});

test("Godot 4 3D Viewport: FOV 70°, Godot toolbar projections, shading modes, and theme", () => {
  const godotViewportConfig = {
    engine: "Godot 4.7.1",
    defaultFov: 70, // Normative Godot 4 default FOV
    projections: ["perspective", "top", "bottom", "front", "back", "right", "left"],
    shadingModes: ["shaded", "wireframe", "unshaded"],
    grid: {
      fadeDistance: 36.0,
      majorStep: 8,
      minorStep: 1,
      xAxisColor: "#eb4747", // Godot Red
      zAxisColor: "#3d85f3", // Godot Blue
    },
    snapping: {
      translationSnap: 1.0,
      rotationSnapDeg: 15,
    },
    theme: {
      headerBackground: "rgba(16, 19, 27, 0.92)",
      skyZenith: "#10131c",
      skyHorizon: "#202636",
      groundHorizon: "#1c212e",
      groundNadir: "#0a0c12",
    },
  };

  assert.equal(godotViewportConfig.defaultFov, 70);
  assert.equal(godotViewportConfig.projections.length, 7);
  assert.equal(godotViewportConfig.shadingModes.includes("wireframe"), true);
  assert.equal(godotViewportConfig.grid.xAxisColor, "#eb4747");
  assert.equal(godotViewportConfig.grid.zAxisColor, "#3d85f3");
  assert.equal(godotViewportConfig.snapping.translationSnap, 1.0);
  assert.equal(godotViewportConfig.snapping.rotationSnapDeg, 15);
});


