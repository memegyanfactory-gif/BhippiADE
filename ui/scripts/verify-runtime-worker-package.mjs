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
];

export async function verifyRuntimeWorkerPackage(distDirectory) {
  const assetsDirectory = resolve(distDirectory, "assets");
  const candidates = (await readdir(assetsDirectory)).filter((name) => WORKER_NAME.test(name));
  if (candidates.length !== 1) {
    throw new Error(`expected exactly one content-hashed runtime worker, found ${candidates.length}`);
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
