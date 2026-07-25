import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = dirname(dirname(fileURLToPath(import.meta.url)));
const workspaceDir = dirname(dirname(appDir));
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host: "))
  ?.slice("host: ".length)
  .trim();

if (!host) throw new Error("Could not determine the Rust host target.");

execFileSync("cargo", ["build", "--release", "-p", "pulse-service", "-p", "pulse-cli"], {
  cwd: workspaceDir,
  stdio: "inherit",
});

const extension = process.platform === "win32" ? ".exe" : "";
const binaries = [
  ["pulse-service", `pulse-service-${host}${extension}`],
  ["pulse", `pulse-${host}${extension}`],
];
for (const [name, targetName] of binaries) {
  const source = join(workspaceDir, "target", "release", `${name}${extension}`);
  const destination = join(appDir, "src-tauri", "binaries", targetName);
  if (!existsSync(source)) throw new Error(`Pulse build did not create ${source}`);
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
}
