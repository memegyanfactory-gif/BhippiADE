/**
 * Build the studio's frontend for `tauri build`.
 *
 * This exists because `npm run build --prefix ../../ui` does not survive a project path
 * containing a space. From `C:\Work\VSCode\Bhippi content\crates\bhippi-app` that prefix
 * resolved to `C:\Work\VSCode\ui` — two levels wrong — and the bundle failed before it
 * started, with an ENOENT for a `package.json` nobody had asked for.
 *
 * Every path here is resolved from `import.meta.url` rather than from the working directory,
 * the way `fetch-godot.mjs` already does it, so it does not matter where Tauri chooses to run
 * the hook from or what the folder is called.
 *
 *   node scripts/build-ui.mjs
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const UI_DIR = path.join(ROOT, "ui");

const manifest = path.join(UI_DIR, "package.json");
if (!fs.existsSync(manifest)) {
  console.error(`build-ui: no package.json at ${manifest}`);
  process.exit(1);
}

// `npm` is a shell script on Windows, so it needs the shell — and the shell is exactly what
// mangles an unquoted path with a space in it. Passing `cwd` instead of `--prefix` keeps the
// path out of the command line altogether.
const result = spawnSync("npm", ["run", "build"], {
  cwd: UI_DIR,
  stdio: "inherit",
  shell: true,
});

if (result.error) {
  console.error(`build-ui: could not start npm: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
