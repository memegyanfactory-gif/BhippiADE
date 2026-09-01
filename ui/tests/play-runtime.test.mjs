import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { PlayRuntime, runScriptedPlaytest } from "../src/engine/playRuntime.ts";

const input = {
  format: "bhippi-input@1",
  actions: [{ name: "jump", keys: ["Space"] }],
  axes: [
    { name: "move_x", positive: ["KeyD"], negative: ["KeyA"] },
    { name: "move_z", positive: ["KeyS"], negative: ["KeyW"] },
  ],
};

const scene = () => ({
  entities: [
    {
      id: "floor",
      name: "Floor",
      tags: [],
      components: {
        Transform: { pos: [0, -0.5, 0], scale: [20, 1, 20] },
        RigidBody: { kind: "static" },
        Collider: { shape: { cuboid: [20, 1, 20] }, sensor: false },
      },
    },
    {
      id: "player",
      name: "Player",
      tags: ["gameplay"],
      components: {
        Transform: { pos: [0, 1, 0], scale: [1, 2, 1] },
        RigidBody: { kind: "dynamic" },
        CharacterController: { height: 1.8, radius: 0.35, step_height: 0.3, move_speed: 4, jump_speed: 5 },
      },
    },
  ],
});

/** Settle the world for a second of simulated time at a fixed 50 ms step. */
const settle = (runtime, seconds = 1) => {
  let frame = null;
  for (let step = 0; step < Math.round(seconds / 0.05); step += 1) frame = runtime.update(0.05);
  return frame;
};

test("play uses named input and component-authored movement values", () => {
  const runtime = new PlayRuntime(scene(), [0, -9.81, 0], input);
  runtime.input.set("KeyD", true);
  const frame = runtime.update(0.25);
  assert.equal(frame.transforms.get("player")[0], 0.2); // delta clamps to 50 ms; speed is 4 m/s
});

test("pause, one-frame step and restart are deterministic", () => {
  const runtime = new PlayRuntime(scene(), [0, -10, 0], input);
  runtime.setPaused(true);
  const paused = runtime.update(0.05);
  const stepped = runtime.update(0.05, 1, true);
  assert.equal(paused.stats.elapsed, 0);
  assert.equal(stepped.stats.elapsed, 0.05);
  runtime.reset();
  runtime.setPaused(true);
  assert.equal(runtime.update(0).transforms.get("player")[1], 1);
});

test("runtime never mutates the authored scene and Stop can discard it exactly", () => {
  const authored = scene();
  const before = JSON.stringify(authored);
  const runtime = new PlayRuntime(authored, [0, -9.81, 0], input);
  runtime.input.set("Space", true);
  runtime.update(0.05);
  assert.equal(JSON.stringify(authored), before);
  assert.equal(runtime.authoredStateUnchanged(authored), true);
});

// -- physics (ENG-172 / ENG-173) ---------------------------------------------------------

test("a falling capsule lands on the floor and stops, rather than tunnelling through it", () => {
  const runtime = new PlayRuntime(scene(), [0, -9.81, 0], input);
  const frame = settle(runtime, 2);
  const y = frame.transforms.get("player")[1];
  // Capsule half-height 0.55 plus radius 0.35 sits its centre 0.9 above a floor topped at 0.
  assert.ok(Math.abs(y - 0.9) < 0.02, `expected the player to rest at ~0.9, got ${y}`);
  assert.ok(frame.stats.contacts > 0, "resting on the floor is a contact");
});

test("jump only fires from the ground, and gravity brings it back", () => {
  const runtime = new PlayRuntime(scene(), [0, -9.81, 0], input);
  settle(runtime, 2);
  const resting = runtime.update(0.05).transforms.get("player")[1];

  runtime.input.set("Space", true);
  const launched = runtime.update(0.05).transforms.get("player")[1];
  assert.ok(launched > resting + 0.1, "a grounded jump must leave the floor");

  // Held mid-air, the same key must not double-jump: `actionPressed` is edge-triggered.
  const peak = settle(runtime, 0.4).transforms.get("player")[1];
  const landed = settle(runtime, 3).transforms.get("player")[1];
  assert.ok(peak > launched);
  assert.ok(Math.abs(landed - 0.9) < 0.02, `expected to land back at ~0.9, got ${landed}`);
});

test("a sphere collider is resolved as a sphere, not as its bounding box", () => {
  const world = {
    entities: [
      {
        id: "floor",
        name: "Floor",
        tags: [],
        components: {
          Transform: { pos: [0, -0.5, 0], scale: [20, 1, 20] },
          RigidBody: { kind: "static" },
          Collider: { shape: { cuboid: [20, 1, 20] } },
        },
      },
      {
        id: "ball",
        name: "Ball",
        tags: [],
        components: {
          Transform: { pos: [0, 5, 0], scale: [1, 1, 1] },
          RigidBody: { kind: "dynamic" },
          Collider: { shape: { sphere: 0.5 } },
        },
      },
    ],
  };
  const runtime = new PlayRuntime(world, [0, -9.81, 0], input);
  const y = settle(runtime, 3).transforms.get("ball")[1];
  assert.ok(Math.abs(y - 0.5) < 0.02, `a 0.5 m sphere rests with its centre at 0.5, got ${y}`);
});

test("a walkable ramp carries the controller; a wall-steep one does not", () => {
  const ramp = (radians) => ({
    entities: [
      {
        id: "ramp",
        name: "Ramp",
        tags: [],
        components: {
          Transform: { pos: [0, 0, 0], rot: [0, 0, radians], scale: [10, 0.5, 10] },
          RigidBody: { kind: "static" },
          Collider: { shape: { cuboid: [10, 0.5, 10] } },
        },
      },
      {
        id: "player",
        name: "Player",
        tags: [],
        components: {
          Transform: { pos: [0, 4, 0], scale: [1, 2, 1] },
          RigidBody: { kind: "dynamic" },
          CharacterController: {
            height: 1.8,
            radius: 0.35,
            step_height: 0.3,
            move_speed: 4,
            jump_speed: 5,
            max_slope: Math.PI / 6, // 30°
          },
        },
      },
    ],
  });

  const gentle = new PlayRuntime(ramp(Math.PI / 9), [0, -9.81, 0], input); // 20° — walkable
  const gentleFrame = settle(gentle, 3);
  assert.equal(gentleFrame.stats.contacts > 0, true);

  const steep = new PlayRuntime(ramp(Math.PI / 3), [0, -9.81, 0], input); // 60° — too steep
  const before = steep.update(0.05).transforms.get("player")[1];
  const after = settle(steep, 2).transforms.get("player")[1];
  assert.ok(after < before, "a body on an unwalkable slope must keep sliding down it");
});

test("a sensor reports a trigger without blocking the body that entered it", () => {
  const world = {
    entities: [
      {
        id: "zone",
        name: "Zone",
        tags: ["exit"],
        components: {
          Transform: { pos: [0, 0, 0], scale: [4, 4, 4] },
          RigidBody: { kind: "static" },
          Collider: { shape: { cuboid: [4, 4, 4] }, sensor: true },
        },
      },
      {
        id: "player",
        name: "Player",
        tags: ["player"],
        components: {
          Transform: { pos: [0, 0, 0], scale: [1, 2, 1] },
          RigidBody: { kind: "dynamic" },
          CharacterController: { height: 1.8, radius: 0.35, move_speed: 4 },
        },
      },
    ],
  };
  const runtime = new PlayRuntime(world, [0, 0, 0], input);
  const frame = runtime.update(0.05);
  assert.ok(frame.events.some((event) => event.kind === "trigger" && event.other === "zone"));
  assert.equal(frame.stats.contacts, 0, "a sensor is not a solid contact");
});

// -- scripts (ENG-176) -------------------------------------------------------------------

const pickupProgram = JSON.parse(
  readFileSync(fileURLToPath(new URL("./fixtures/pickup.program.json", import.meta.url)), "utf8"),
);

const pickupWorld = () => ({
  entities: [
    {
      id: "pickup",
      name: "Coin",
      tags: ["pickup"],
      components: {
        Transform: { pos: [0, 1, 0], scale: [1, 1, 1] },
        RigidBody: { kind: "static" },
        Collider: { shape: { cuboid: [1, 1, 1] }, sensor: true },
        ScriptRef: { script: "assets/scripts/pickup.rhai" },
      },
    },
    {
      id: "player",
      name: "Player",
      tags: ["player"],
      components: {
        Transform: { pos: [0, 1, 0], scale: [1, 2, 1] },
        RigidBody: { kind: "dynamic" },
        CharacterController: { height: 1.8, radius: 0.35, move_speed: 4 },
      },
    },
  ],
});

test("a program compiled by Rust drives the runtime end to end", () => {
  const runtime = new PlayRuntime(pickupWorld(), [0, 0, 0], input, {
    scripts: new Map([["pickup", pickupProgram]]),
  });
  assert.deepEqual(runtime.unboundHosts(), [], "every host the fixture calls must be implemented");

  const frame = runtime.update(0.05);

  // on_start ran: the score is seeded and the HUD label written.
  assert.equal(frame.variables["game.score"], 10, "on_trigger already scored this frame");
  assert.equal(frame.hud.score_label, "Score 10");
  // on_trigger fired from the sensor overlap, then destroyed the pickup.
  assert.ok(frame.removed.includes("pickup"));
  assert.ok(frame.events.some((event) => event.kind === "sound" && event.asset.endsWith("pickup.ogg")));
  assert.equal(frame.stats.scriptFaults, 0);
});

test("on_update moves the entity the script owns, deterministically", () => {
  // No sensor overlap this time, so the pickup survives and only on_update runs.
  const world = pickupWorld();
  world.entities[1].components.Transform.pos = [20, 1, 0];
  const first = new PlayRuntime(world, [0, 0, 0], input, { scripts: new Map([["pickup", pickupProgram]]) });
  const second = new PlayRuntime(pickupWorld(), [0, 0, 0], input, {
    scripts: new Map([["pickup", pickupProgram]]),
  });
  second.setVariable("game.score", 0);

  const a = settle(first, 0.5).transforms.get("pickup");
  assert.ok(Math.abs(a[1] - 1) > 0.001, "the bob in on_update must move the entity");

  const replay = new PlayRuntime(
    (() => {
      const w = pickupWorld();
      w.entities[1].components.Transform.pos = [20, 1, 0];
      return w;
    })(),
    [0, 0, 0],
    input,
    { scripts: new Map([["pickup", pickupProgram]]) },
  );
  const b = settle(replay, 0.5).transforms.get("pickup");
  assert.deepEqual(a, b, "the same inputs must produce the same frame");
});

test("a script fault is located, disables that script only, and can pause play", () => {
  const broken = {
    ...pickupProgram,
    // Divide by zero on line 9, in on_update.
    code: [
      { op: "push_num", a: 0, b: 0, line: 9 },
      { op: "push_num", a: 1, b: 0, line: 9 },
      { op: "div", a: 0, b: 0, line: 9 },
      { op: "return", a: 0, b: 0, line: 9 },
    ],
    numbers: [1, 0],
    strings: [],
    hosts: [],
    functions: [{ name: "on_update", entry: 0, params: 1, locals: 1, line: 9 }],
    hooks: [{ hook: "on_update", function: 0 }],
  };
  const runtime = new PlayRuntime(pickupWorld(), [0, 0, 0], input, {
    scripts: new Map([["pickup", broken]]),
    pauseOnError: true,
  });

  const frame = runtime.update(0.05);
  const fault = frame.events.find((event) => event.kind === "script_fault");
  assert.ok(fault, "the fault must reach the frame so the Output Log can show it");
  assert.equal(fault.line, 9);
  assert.equal(fault.entity, "pickup");
  assert.equal(frame.stats.scriptFaults, 1);
  assert.equal(runtime.isPaused(), true, "pause-on-error stops on the frame that broke");

  // The same fault must not repeat every frame.
  const next = runtime.update(0.05, 1, true);
  assert.equal(next.events.some((event) => event.kind === "script_fault"), false);
});

test("stats report what the transport bar shows", () => {
  const runtime = new PlayRuntime(pickupWorld(), [0, -9.81, 0], input, {
    scripts: new Map([["pickup", pickupProgram]]),
  });
  const frame = settle(runtime, 0.5);
  // The pickup destroyed itself on the first frame, so the entity count is the live world's,
  // not the authored one's — which is the number the transport bar is supposed to show.
  assert.equal(frame.stats.entities, 1);
  assert.equal(frame.stats.simulatedBodies, 1, "only the player is non-static");
  assert.equal(frame.stats.scripts, 0, "the destroyed entity's VM went with it");
  assert.ok(frame.stats.fps > 0);
  assert.ok(frame.stats.frameMs > 0);
});

test("a scripted agent playtest is deterministic and leaves authored state byte-identical", () => {
  const authored = scene();
  const before = JSON.stringify(authored);
  const steps = [
    { keys: ["KeyW"], frames: 30, note: "walk" },
    { keys: [], frames: 30, note: "settle" },
  ];
  const first = runScriptedPlaytest(authored, [0, -9.81, 0], input, new Map(), steps, 1 / 60);
  const second = runScriptedPlaytest(authored, [0, -9.81, 0], input, new Map(), steps, 1 / 60);
  assert.deepEqual(first, second);
  assert.equal(first.authoredUnchanged, true);
  assert.equal(first.completed, true);
  assert.equal(first.authoredHashBefore, first.authoredHashAfter);
  assert.equal(first.frames, 60);
  assert.equal(first.samples.length, 2);
  assert.equal(JSON.stringify(authored), before);
});

test("a fall-through checkpoint fails, then the same playtest passes after adding a collider", () => {
  const broken = scene();
  broken.entities = broken.entities.filter((entity) => entity.name !== "Floor");
  const steps = [{ keys: [], frames: 120, note: "settle on warehouse floor" }];
  const failed = runScriptedPlaytest(broken, [0, -9.81, 0], input, new Map(), steps, 1 / 60);
  const failedY = failed.samples.at(-1).transforms.player[1];
  assert.ok(failedY < -1, `expected a fall-through coordinate, received ${failedY}`);

  const repaired = runScriptedPlaytest(scene(), [0, -9.81, 0], input, new Map(), steps, 1 / 60);
  const repairedY = repaired.samples.at(-1).transforms.player[1];
  assert.ok(repairedY >= 0.89, `same verifier must see the repaired floor, received ${repairedY}`);
  assert.equal(repaired.authoredHashBefore, repaired.authoredHashAfter);
});
