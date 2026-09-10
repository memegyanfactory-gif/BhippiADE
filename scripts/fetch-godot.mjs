/**
 * Put the pinned Godot into `crates/bhippi-app/resources/godot/` so `tauri build` bundles it
 * (ADR-0047).
 *
 * The binary is ~120 MB and does not belong in git, so the repository carries this script and
 * the checksum instead. It is idempotent: a resource directory that already holds the right
 * build is left alone, which is what makes it safe to hang off `beforeBuildCommand`.
 *
 *   node scripts/fetch-godot.mjs            # fetch for this platform if missing
 *   node scripts/fetch-godot.mjs --check    # exit 1 if the bundle is missing; fetch nothing
 *   node scripts/fetch-godot.mjs --force    # re-fetch even if it looks complete
 *
 * The version, the archive names and the licence text all follow `GODOT_PINNED_TAG` in
 * `crates/bhippi-engine/src/godot/detect.rs`. Moving one means moving all of them, which is
 * the release step ADR-0047 describes.
 */

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const OUT_DIR = path.join(ROOT, "crates", "bhippi-app", "resources", "godot");

/** Kept in step with `GODOT_PINNED_TAG`. */
const TAG = "4.7.1-stable";
const BASE = `https://github.com/godotengine/godot/releases/download/${TAG}/`;

/**
 * What each platform downloads and what the archive should unpack to. `expect` is the file
 * detection looks for; on Windows the console build comes in the same archive and is what
 * `--version` is asked, so both halves matter.
 */
const TARGETS = {
  win32: {
    archive: `Godot_v${TAG}_win64.exe.zip`,
    expect: [`Godot_v${TAG}_win64.exe`, `Godot_v${TAG}_win64_console.exe`],
  },
  darwin: {
    archive: `Godot_v${TAG}_macos.universal.zip`,
    expect: ["Godot.app"],
  },
  linux: {
    archive: `Godot_v${TAG}_linux.x86_64.zip`,
    expect: [`Godot_v${TAG}_linux.x86_64`],
  },
};

/**
 * SHA-256 of each archive, filled in on the first successful fetch and committed.
 *
 * Empty means "not recorded yet": the script prints the digest it saw and tells you to paste
 * it here. It refuses to *silently* accept a mismatch once a value is present — that is the
 * whole point of writing it down.
 */
const SHA256 = {
  [`Godot_v${TAG}_win64.exe.zip`]: "c7a289051eaefb460b0106b60e9cd5bee0ef55fd102dcb2bed1eb356cf3d90a1",
  [`Godot_v${TAG}_macos.universal.zip`]: "",
  [`Godot_v${TAG}_linux.x86_64.zip`]: "",
};

const args = new Set(process.argv.slice(2));
const platform = process.platform === "darwin" ? "darwin" : process.platform === "win32" ? "win32" : "linux";
const target = TARGETS[platform];

function log(message) {
  process.stdout.write(`fetch-godot: ${message}\n`);
}

function fail(message) {
  process.stderr.write(`fetch-godot: ${message}\n`);
  process.exit(1);
}

/** True when every file the archive should have produced is present and non-empty. */
function bundleLooksComplete() {
  return target.expect.every((name) => {
    const at = path.join(OUT_DIR, name);
    if (!fs.existsSync(at)) return false;
    const stat = fs.statSync(at);
    return stat.isDirectory() || stat.size > 0;
  });
}

if (args.has("--check")) {
  if (bundleLooksComplete()) {
    log(`bundle present in ${path.relative(ROOT, OUT_DIR)}`);
    process.exit(0);
  }
  fail(
    `no bundled Godot in ${path.relative(ROOT, OUT_DIR)}.\n` +
      "  Run: node scripts/fetch-godot.mjs\n" +
      "  A packaged build without it would ship with no engine (ADR-0047).",
  );
}

if (bundleLooksComplete() && !args.has("--force")) {
  log(`already present in ${path.relative(ROOT, OUT_DIR)} — nothing to do`);
  process.exit(0);
}

const url = `${BASE}${target.archive}`;
log(`downloading ${url}`);

const response = await fetch(url, { redirect: "follow" });
if (!response.ok) {
  fail(`GitHub answered ${response.status} ${response.statusText} for ${url}`);
}
const buffer = Buffer.from(await response.arrayBuffer());
const digest = createHash("sha256").update(buffer).digest("hex");

const expected = SHA256[target.archive];
if (!expected) {
  log(`sha256 ${digest}`);
  log(`no checksum recorded for ${target.archive} yet — paste the digest above into SHA256 in this script`);
} else if (expected !== digest) {
  fail(
    `checksum mismatch for ${target.archive}\n` +
      `  expected ${expected}\n` +
      `  got      ${digest}\n` +
      "  Refusing to unpack. Either the release was re-cut or the download is not what it claims.",
  );
} else {
  log(`sha256 ok (${digest.slice(0, 16)}…)`);
}

fs.mkdirSync(OUT_DIR, { recursive: true });
const archivePath = path.join(OUT_DIR, target.archive);
fs.writeFileSync(archivePath, buffer);
log(`unpacking ${target.archive}`);

// No unzip dependency: every platform this ships on already has one in the box.
const unzip =
  platform === "win32"
    ? spawnSync(
        "powershell",
        [
          "-NoProfile",
          "-NonInteractive",
          "-Command",
          `Expand-Archive -LiteralPath '${archivePath}' -DestinationPath '${OUT_DIR}' -Force`,
        ],
        { stdio: "inherit" },
      )
    : spawnSync("unzip", ["-o", archivePath, "-d", OUT_DIR], { stdio: "inherit" });

if (unzip.status !== 0) {
  fail(`could not unpack ${target.archive}`);
}
fs.rmSync(archivePath, { force: true });

if (!bundleLooksComplete()) {
  fail(
    `unpacked ${target.archive} but ${target.expect.join(", ")} is still missing — ` +
      "the release layout may have changed",
  );
}

log(`ready: ${target.expect.join(", ")}`);
