#!/usr/bin/env node
const path = require("path");
const fs = require("fs");
const { spawn } = require("child_process");

function getPlatformInfo() {
  const p = process.platform;
  const a = process.arch;
  const pm = { win32: "windows", darwin: "macos", linux: "linux" };
  const am = { x64: "x64", arm64: "arm64" };
  return {
    platform: pm[p] || p,
    arch: am[a] || a,
    ext: p === "win32" ? ".exe" : "",
    isWin: p === "win32",
  };
}

function findBinary(binDir) {
  const { platform, arch, ext } = getPlatformInfo();

  const candidates = [
    `rds-${platform}-${arch}${ext}`,
    `rds-${platform}${ext}`,
    `rds${ext}`,
  ];

  for (const name of candidates) {
    const p = path.join(binDir, name);
    if (fs.existsSync(p)) return p;
  }

  // fallback: any rds* binary in bin/
  try {
    const files = fs.readdirSync(binDir);
    for (const f of files) {
      if (f.startsWith("rds")) {
        const p = path.join(binDir, f);
        if (fs.statSync(p).isFile()) return p;
      }
    }
  } catch {
    // ignore
  }

  return null;
}

function main() {
  const binDir = path.join(__dirname);
  const binary = findBinary(binDir);
  if (!binary) {
    console.error("rds binary not found in package bin/ directory.");
    console.error(
      "Please run `npm run build` or `cargo build --release` and copy the binary to bin/."
    );
    process.exit(1);
  }

  // Ensure executable on non-windows
  if (!getPlatformInfo().isWin) {
    try {
      fs.chmodSync(binary, 0o755);
    } catch {
      /* ignore */
    }
  }

  const args = process.argv.slice(2);

  const child = spawn(binary, args, {
    stdio: "inherit",
    env: process.env,
  });

  child.on("close", (code) => process.exit(code));
  child.on("error", (err) => {
    console.error(
      "Failed to spawn rds binary:",
      err && err.message ? err.message : err
    );
    process.exit(1);
  });
}

if (require.main === module) main();
