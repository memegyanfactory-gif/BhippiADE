/**
 * The host -> runtime-capability mapping exists in three places:
 *
 *   1. `crates/bhippi-engine/src/runtime_protocol.rs`  (`RuntimeCapability::for_script_host`)
 *   2. `ui/src/engine/runtimeWorkerSession.ts`         (`HOST_CAPABILITY`)
 *   3. `ui/src/engine/playRuntime.ts`                  (`BROKERED_HOST_CAPABILITY`)
 *
 * Rust owns it. The two TypeScript copies are mirrors, and until this test existed nothing
 * checked that they agreed. That is not a theoretical risk: a grant that widens in one copy and
 * not the others is invisible, and the copy that stays wide is the one that actually decides
 * whether a generated mechanic can delete an entity (`playRuntime` deletes ungranted hosts from
 * the VM's function table).
 *
 * This reads all three as text rather than importing them, so it needs no build step and cannot
 * be fooled by a TypeScript type that says one thing while the literal says another.
 */

import { strict as assert } from "node:assert";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import test from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..");

const read = (...parts) => readFileSync(join(repoRoot, ...parts), "utf8");

/** Rust: parse the `match host { "a" | "b" => Some(Self::Cap), ... }` arms. */
function rustMapping() {
  const source = read("crates", "bhippi-engine", "src", "runtime_protocol.rs");
  const body = source.slice(
    source.indexOf("pub fn for_script_host("),
    source.indexOf("/// Canonical Rust-owned grant set"),
  );
  assert.ok(body.length > 0, "could not locate for_script_host in the Rust source");

  // `as_str` is the wire name; the mirrors key off that, not the Rust identifier.
  const wireNames = new Map();
  const asStr = source.slice(source.indexOf("pub const fn as_str("), source.indexOf("pub fn for_script_host("));
  for (const [, variant, wire] of asStr.matchAll(/Self::(\w+)\s*=>\s*"([a-z_]+)"/g)) {
    wireNames.set(variant, wire);
  }
  assert.ok(wireNames.size >= 8, `expected the full capability set, saw ${wireNames.size}`);

  const mapping = new Map();
  // Arms look like:  "a" | "b" => Some(Self::Cap),   or   "a" | "b" => { Some(Self::Cap) }
  for (const [, hosts, variant] of body.matchAll(
    /((?:\s*"[a-z_0-9]+"\s*\|?)+)\s*=>\s*\{?\s*Some\(Self::(\w+)\)/g,
  )) {
    const wire = wireNames.get(variant);
    assert.ok(wire, `no wire name for Self::${variant}`);
    for (const [, host] of hosts.matchAll(/"([a-z_0-9]+)"/g)) mapping.set(host, wire);
  }
  return mapping;
}

/** TypeScript: parse an object literal of `host: "capability"` pairs. */
function tsMapping(relativePath, constName) {
  const source = read(...relativePath);
  const start = source.indexOf(`const ${constName}`);
  assert.ok(start >= 0, `${constName} not found in ${relativePath.join("/")}`);
  const open = source.indexOf("{", start);
  const close = source.indexOf("};", open);
  assert.ok(close > open, `${constName} literal is not terminated`);
  const body = source.slice(open, close);

  const mapping = new Map();
  for (const [, host, capability] of body.matchAll(/(\w+)\s*:\s*"([a-z_]+)"/g)) {
    mapping.set(host, capability);
  }
  return mapping;
}

test("every runtime host maps to the same capability in Rust and both TypeScript mirrors", () => {
  const rust = rustMapping();
  const worker = tsMapping(["ui", "src", "engine", "runtimeWorkerSession.ts"], "HOST_CAPABILITY");
  const runtime = tsMapping(["ui", "src", "engine", "playRuntime.ts"], "BROKERED_HOST_CAPABILITY");

  assert.ok(rust.size >= 30, `expected the full host table from Rust, saw ${rust.size}`);

  const disagreements = [];
  const hosts = new Set([...rust.keys(), ...worker.keys(), ...runtime.keys()]);
  for (const host of [...hosts].sort()) {
    const expected = rust.get(host);
    if (expected === undefined) {
      // A host Rust does not grant is worker-local (pure maths, logging). Neither mirror may
      // invent a capability for it.
      if (worker.has(host) || runtime.has(host)) {
        disagreements.push(`${host}: Rust grants nothing, mirrors claim ${worker.get(host) ?? "-"}/${runtime.get(host) ?? "-"}`);
      }
      continue;
    }
    if (worker.get(host) !== expected) {
      disagreements.push(`${host}: Rust says ${expected}, runtimeWorkerSession says ${worker.get(host) ?? "missing"}`);
    }
    if (runtime.get(host) !== expected) {
      disagreements.push(`${host}: Rust says ${expected}, playRuntime says ${runtime.get(host) ?? "missing"}`);
    }
  }

  assert.deepEqual(disagreements, [], `host capability mirrors drifted:\n${disagreements.join("\n")}`);
});

test("moving an entity never confers the power to spawn or destroy one", () => {
  // The concrete least-privilege boundary, asserted on the mirrors the runtime actually reads
  // rather than only on the Rust that derives the grant.
  for (const [file, constName] of [
    [["ui", "src", "engine", "runtimeWorkerSession.ts"], "HOST_CAPABILITY"],
    [["ui", "src", "engine", "playRuntime.ts"], "BROKERED_HOST_CAPABILITY"],
  ]) {
    const mapping = tsMapping(file, constName);
    const movement = mapping.get("set_vel");
    const spawn = mapping.get("spawn");
    const destroy = mapping.get("destroy");

    assert.ok(movement, `${constName} must map set_vel`);
    assert.notEqual(
      spawn,
      movement,
      `${constName}: spawn shares a grant with set_vel, so any mechanic that moves an entity can create one`,
    );
    assert.notEqual(
      destroy,
      movement,
      `${constName}: destroy shares a grant with set_vel, so any mechanic that moves an entity can delete one`,
    );
    assert.equal(spawn, destroy, `${constName}: spawn and destroy are one lifecycle grant`);
  }
});
