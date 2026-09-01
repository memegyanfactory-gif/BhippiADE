import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { verifyRuntimeWorkerPackage } from "../scripts/verify-runtime-worker-package.mjs";

test("packaged runtime worker has a reproducible provenance record", async () => {
  const root = await mkdtemp(join(tmpdir(), "bhippi-worker-package-"));
  try {
    const assets = join(root, "assets");
    await mkdir(assets);
    await writeFile(join(assets, "playRuntime.worker-AbC_123.js"), "self.onmessage=()=>{};\n");

    const first = await verifyRuntimeWorkerPackage(root);
    const second = await verifyRuntimeWorkerPackage(root);
    assert.deepEqual(first, second);
    assert.equal(first.bundle, "assets/playRuntime.worker-AbC_123.js");
    assert.match(first.sha256, /^[a-f0-9]{64}$/);
    assert.equal(first.csp, "worker-src 'self'");
    assert.deepEqual(
      JSON.parse(await readFile(join(root, "runtime-worker-provenance.json"), "utf8")),
      first,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("packaging rejects ambiguous or ambient-authority worker bundles", async () => {
  const root = await mkdtemp(join(tmpdir(), "bhippi-worker-package-"));
  try {
    const assets = join(root, "assets");
    await mkdir(assets);
    await assert.rejects(verifyRuntimeWorkerPackage(root), /exactly one/);
    await writeFile(join(assets, "playRuntime.worker-bad.js"), "fetch('https://example.com');\n");
    await assert.rejects(verifyRuntimeWorkerPackage(root), /forbidden authority/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Tauri CSP permits only application-owned scripts and workers", async () => {
  const config = JSON.parse(
    await readFile(new URL("../../crates/bhippi-app/tauri.conf.json", import.meta.url), "utf8"),
  );
  const csp = config.app.security.csp;
  assert.equal(csp["script-src"], "'self'");
  assert.equal(csp["worker-src"], "'self'");
  assert.equal(csp["object-src"], "'none'");
  assert.doesNotMatch(csp["script-src"], /unsafe-eval|https?:|blob:|data:/);
  assert.doesNotMatch(csp["worker-src"], /unsafe-eval|https?:|blob:|data:/);
});
