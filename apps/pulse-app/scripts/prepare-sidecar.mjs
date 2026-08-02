import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = dirname(dirname(fileURLToPath(import.meta.url)));
const workspaceDir = dirname(dirname(appDir));

function stopOrphanedDebugSidecar() {
  if (process.platform !== "win32") return;

  const debugService = join(workspaceDir, "target", "debug", "pulse-service.exe");
  const debugApp = join(workspaceDir, "target", "debug", "pulse-app.exe");
  const script = `
    $service = [IO.Path]::GetFullPath($env:PULSE_DEBUG_SERVICE)
    $app = [IO.Path]::GetFullPath($env:PULSE_DEBUG_APP)
    Get-CimInstance Win32_Process -Filter "Name = 'pulse-service.exe'" |
      Where-Object {
        $_.ExecutablePath -and
        [IO.Path]::GetFullPath($_.ExecutablePath) -ieq $service
      } |
      ForEach-Object {
        $parent = Get-CimInstance Win32_Process -Filter "ProcessId = $($_.ParentProcessId)" -ErrorAction SilentlyContinue
        if (-not $parent -or -not $parent.ExecutablePath -or [IO.Path]::GetFullPath($parent.ExecutablePath) -ine $app) {
          Stop-Process -Id $_.ProcessId -Force
          Write-Output "Stopped orphaned debug pulse-service (PID $($_.ProcessId))."
        }
      }
  `;
  const output = execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    encoding: "utf8",
    env: { ...process.env, PULSE_DEBUG_SERVICE: debugService, PULSE_DEBUG_APP: debugApp },
  }).trim();
  if (output) console.log(output);
}

stopOrphanedDebugSidecar();

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
