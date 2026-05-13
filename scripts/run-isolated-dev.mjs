import { spawn } from "node:child_process";
import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import os from "node:os";

const projectRoot = process.cwd();
const isolatedDataDir = join(os.tmpdir(), "forisfstools-isolated-runtime");

if (existsSync(isolatedDataDir)) {
  rmSync(isolatedDataDir, { recursive: true, force: true });
}

const env = {
  ...process.env,
  FORISFSTOOLS_ISOLATED: "1",
  FORISFSTOOLS_DATA_DIR: isolatedDataDir,
  FORISFSTOOLS_PROJECT_ROOT: projectRoot,
};

console.log("[forisfstools:isolated] data dir =", isolatedDataDir);
console.log("[forisfstools:isolated] mode = FORISFSTOOLS_ISOLATED=1");

const child = spawn("npm", ["run", "tauri", "dev"], {
  cwd: projectRoot,
  stdio: "inherit",
  env,
  shell: process.platform === "win32",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
