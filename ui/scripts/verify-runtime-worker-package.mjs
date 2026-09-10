import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const WORKER_NAME = /^playRuntime\.worker-[A-Za-z0-9_-]+\.js$/;
const FORBIDDEN_RUNTIME_AUTHORITY = [
  /\bfetch\s*\(/,
  /\bXMLHttpRequest\b/,
  /\bWebSocket\b/,
  /\bimportScripts\b/,
  /\beval\s*\(/,
  /\bnew\s+Function\b/,
  /\bimport\s*\(/,
  /\b__TAURI(?:_INTERNALS__)?\b/,
  /@tauri-apps/,
  /\binvoke\s*\(/,
  /\b(?:globalThis|self)\.document\b/,
  /\bdocument\.(?:body|cookie|createElement|querySelector|getElementById)\b/,
  /\bwindow\b/,
  /\blocalStorage\b/,
  /\bindexedDB\b/,
  /\bnavigator\b/,
  /\bprocess\s*\./,
  /\brequire\s*\(/,
];

export async function verifyRuntimeWorkerPackage(distDirectory) {
  const assetsDirectory = resolve(distDirectory, "assets");
  const candidates = (await readdir(assetsDirectory)).filter((name) => WORKER_NAME.test(name));
  if (candidates.length > 1) {
    throw new Error(`expected at most one content-hashed runtime worker, found ${candidates.length}`);
  }
  // ADR-0043 made Godot the runtime, so the workbench's Engine mode mounts the Godot pane
  // and the in-webview gameplay worker (ADR-0033) is no longer reachable from the entry —
  // Rollup drops it, and there is nothing in `dist` to check. The gate keeps every tooth it
  // had over a worker that *does* ship: more than one is still a hard failure, and the
  // forbidden-authority scan below still blocks. What is written instead is a provenance
  // file that says, in the artefact itself, that the surface is gone. `ui/src/engine/**`
  // is deleted in Phase G5 and this script goes with it.
  if (candidates.length === 0) {
    const provenance = {
      format: "bhippi-runtime-worker-provenance@1",
      source: "src/engine/playRuntime.worker.ts",
      bundle: null,
      sha256: null,
      csp: "worker-src 'self'",
      retired: "ADR-0043: Godot is the runtime; the webview gameplay worker no longer ships",
    };
    await writeFile(
      resolve(distDirectory, "runtime-worker-provenance.json"),
      `${JSON.stringify(provenance, null, 2)}\n`,
      "utf8",
    );
    return provenance;
  }

  const workerName = candidates[0];
  const workerBytes = await readFile(resolve(assetsDirectory, workerName));
  const workerSource = workerBytes.toString("utf8");
  for (const forbidden of FORBIDDEN_RUNTIME_AUTHORITY) {
    if (forbidden.test(workerSource)) {
      throw new Error(`runtime worker bundle contains forbidden authority: ${forbidden.source}`);
    }
  }

  const provenance = {
    format: "bhippi-runtime-worker-provenance@1",
    source: "src/engine/playRuntime.worker.ts",
    bundle: `assets/${workerName}`,
    sha256: createHash("sha256").update(workerBytes).digest("hex"),
    csp: "worker-src 'self'",
  };
  await writeFile(
    resolve(distDirectory, "runtime-worker-provenance.json"),
    `${JSON.stringify(provenance, null, 2)}\n`,
    "utf8",
  );
  return provenance;
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : "";
if (import.meta.url === invokedPath) {
  await verifyRuntimeWorkerPackage(fileURLToPath(new URL("../dist", import.meta.url)));
}
